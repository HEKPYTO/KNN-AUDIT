use crate::compare::{self, Comparison, RankSwap};
use crate::dataset::{Dataset, DatasetError};
use crate::report::{
    CategorySummary, DatasetInfo, EngineInfo, Failure, Report, RunSummary, Summary,
};
use chrono::{SecondsFormat, Utc};
use knn_audit_adapters::Engine;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct RunOptions {
    pub dataset: Dataset,
    pub engine: Box<dyn Engine + Send + Sync>,
    pub k: usize,
    pub limit: Option<usize>,
    pub repeat: usize,
}

pub struct RunResult {
    pub report: Report,
    pub summary_line: String,
}

struct Outcome {
    category: String,
    failures: Vec<Option<Failure>>,
    actual_sets: Vec<Vec<u64>>,
    unstable: bool,
}

pub fn run(options: RunOptions) -> Result<RunResult, DatasetError> {
    if options.k == 0 {
        return Err(DatasetError::new("k must be positive"));
    }
    if options.repeat == 0 {
        return Err(DatasetError::new("repeat must be positive"));
    }
    let dataset = options.dataset;
    let cases: Vec<_> = dataset
        .cases
        .iter()
        .take(options.limit.unwrap_or(dataset.cases.len()))
        .collect();
    let expected: Vec<Vec<u64>> = cases
        .iter()
        .map(|case| dataset.expected_ids(case, options.k))
        .collect::<Result<_, _>>()?;
    let collection = dataset.collection();
    let mut engine: Arc<dyn Engine + Send + Sync> = Arc::from(options.engine);
    let mut outcomes: Vec<Outcome> = Vec::new();
    let mut runs: Vec<RunSummary> = Vec::with_capacity(options.repeat);

    for repetition in 0..options.repeat {
        let prepared = Arc::get_mut(&mut engine)
            .ok_or_else(|| DatasetError::new("engine is shared before prepare"))?
            .prepare(&collection);
        if let Err(error) = prepared {
            return Err(DatasetError::new(format!(
                "{} prepare failed: {error}",
                engine.name()
            )));
        }
        let shared = Arc::clone(&engine);
        let results: Vec<(String, Option<Failure>, Vec<u64>)> = cases
            .par_iter()
            .zip(expected.par_iter())
            .map(|(case, expected_ids)| {
                let (actual_ids, failure) = match shared.search(&case.query, options.k) {
                    Ok(hits) => match parse_ids(&hits) {
                        Ok(ids) => {
                            let comparison =
                                compare::compare(&case.query, &collection, expected_ids, &ids);
                            let failure = build_failure(case, expected_ids, &ids, comparison, None);
                            (ids, failure)
                        }
                        Err(message) => {
                            let comparison =
                                compare::compare(&case.query, &collection, expected_ids, &[]);
                            (
                                Vec::new(),
                                build_failure(case, expected_ids, &[], comparison, Some(message)),
                            )
                        }
                    },
                    Err(error) => {
                        let comparison =
                            compare::compare(&case.query, &collection, expected_ids, &[]);
                        (
                            Vec::new(),
                            build_failure(case, expected_ids, &[], comparison, Some(error.message)),
                        )
                    }
                };
                (case.category.clone(), failure, actual_ids)
            })
            .collect();
        if repetition == 0 {
            outcomes = results
                .iter()
                .map(|(category, ..)| Outcome {
                    category: category.clone(),
                    failures: Vec::with_capacity(options.repeat),
                    actual_sets: Vec::new(),
                    unstable: false,
                })
                .collect();
        }
        let mut failed = 0;
        let mut missing = 0;
        let mut extra = 0;
        let mut rank_swaps = 0;
        let mut errors = 0;
        for (outcome, (_, failure, actual_ids)) in outcomes.iter_mut().zip(results) {
            outcome.failures.push(failure.clone());
            if !outcome
                .actual_sets
                .iter()
                .any(|seen| same_set(seen, &actual_ids))
            {
                outcome.actual_sets.push(actual_ids);
            }
            if let Some(item) = failure {
                failed += 1;
                missing += item.missing_ids.len();
                extra += item.extra_ids.len();
                rank_swaps += item.rank_swaps.len();
                if item.error.is_some() {
                    errors += 1;
                }
            }
        }
        runs.push(RunSummary {
            failed,
            errors,
            missing,
            extra,
            rank_swaps,
        });
    }

    for outcome in &mut outcomes {
        outcome.unstable = outcome.actual_sets.len() > 1;
    }
    let unstable = outcomes.iter().filter(|outcome| outcome.unstable).count();

    let mut failures: Vec<Failure> = Vec::new();
    for outcome in &outcomes {
        let failed_repetitions: Vec<usize> = outcome
            .failures
            .iter()
            .enumerate()
            .filter_map(|(index, failure)| failure.as_ref().map(|_| index + 1))
            .collect();
        if let Some(first) = outcome.failures.iter().flatten().next() {
            let mut failure = first.clone();
            failure.failed_repetitions = failed_repetitions;
            failure.unstable = outcome.unstable;
            if outcome.unstable {
                failure.unstable_actual_ids = outcome.actual_sets.clone();
            }
            failures.push(failure);
        }
    }
    failures.sort_by(|left, right| {
        left.case_id
            .cmp(&right.case_id)
            .then_with(|| left.category.cmp(&right.category))
    });
    let mut category_counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for outcome in &outcomes {
        let entry = category_counts
            .entry(outcome.category.clone())
            .or_insert((0, 0));
        entry.0 += 1;
        if outcome.failures.iter().any(Option::is_some) {
            entry.1 += 1;
        }
    }
    let categories: Vec<CategorySummary> = category_counts
        .into_iter()
        .map(|(category, (count, failed))| CategorySummary {
            category,
            cases: count,
            failed,
        })
        .collect();
    let missing: usize = failures
        .iter()
        .map(|failure| failure.missing_ids.len())
        .sum();
    let extra: usize = failures.iter().map(|failure| failure.extra_ids.len()).sum();
    let rank_swaps: usize = failures
        .iter()
        .map(|failure| failure.rank_swaps.len())
        .sum();
    let errors: usize = failures
        .iter()
        .filter(|failure| failure.error.is_some())
        .count();
    let cases_count = outcomes.len();
    let failed = outcomes
        .iter()
        .filter(|outcome| outcome.failures.iter().any(Option::is_some))
        .count();
    let failed_all = outcomes
        .iter()
        .filter(|outcome| outcome.failures.iter().all(Option::is_some))
        .count();
    let summary = Summary {
        cases: cases_count,
        passed: cases_count - failed,
        failed,
        errors,
        missing,
        extra,
        rank_swaps,
        unstable,
        failed_all,
    };
    let name = engine.name().to_string();
    let version = engine.version();
    let capabilities = engine.capabilities();
    let report = Report {
        schema: crate::report::SCHEMA.to_string(),
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        dataset: DatasetInfo {
            root: dataset.root.clone(),
            base_sha256: dataset.base_sha256.clone(),
            base_rows: dataset.base_rows,
            cases: dataset.cases.len(),
            dim: dataset.dim,
        },
        engine: EngineInfo {
            name: name.clone(),
            version: version.clone(),
            exact_mode: capabilities.exact_mode,
            metric: capabilities.distance_metric.as_str().to_string(),
            config_redacted: engine.config(),
        },
        k: options.k,
        summary,
        categories,
        failures,
        repeat: options.repeat,
        runs,
    };
    let summary_line = format!(
        "{name} {version} (repeat {}): {failed}/{cases_count} cases mismatch any ({failed_all} in every run, {unstable} unstable; {errors} errors; {missing} missing, {extra} extra, {rank_swaps} rank swaps)",
        options.repeat
    );
    Ok(RunResult {
        report,
        summary_line,
    })
}

fn same_set(left: &[u64], right: &[u64]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_unstable();
    right.sort_unstable();
    left == right
}

fn parse_ids(hits: &[knn_audit_adapters::Hit]) -> Result<Vec<u64>, String> {
    hits.iter()
        .map(|hit| {
            hit.id
                .parse::<u64>()
                .map_err(|_| format!("engine returned non-numeric id {:?}", hit.id))
        })
        .collect()
}

fn build_failure(
    case: &crate::dataset::Case,
    expected_ids: &[u64],
    actual_ids: &[u64],
    comparison: Comparison,
    error: Option<String>,
) -> Option<Failure> {
    if error.is_none()
        && comparison.missing_ids.is_empty()
        && comparison.extra_ids.is_empty()
        && comparison.rank_swaps.is_empty()
    {
        return None;
    }
    let rank_swaps: Vec<RankSwap> = comparison.rank_swaps.into_iter().collect();
    Some(Failure {
        case_id: case.case_id.clone(),
        category: case.category.clone(),
        query: case.query.clone(),
        expected_ids: expected_ids.to_vec(),
        actual_ids: actual_ids.to_vec(),
        missing_ids: comparison.missing_ids,
        extra_ids: comparison.extra_ids,
        rank_swaps,
        distance_gap: comparison.distance_gap,
        error,
        failed_repetitions: Vec::new(),
        unstable: false,
        unstable_actual_ids: Vec::new(),
    })
}
