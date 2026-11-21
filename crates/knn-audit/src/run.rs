use crate::compare::{self, Comparison, RankSwap};
use crate::dataset::{Dataset, DatasetError};
use crate::report::{CategorySummary, DatasetInfo, EngineInfo, Failure, Report, Summary};
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
}

pub struct RunResult {
    pub report: Report,
    pub summary_line: String,
}

struct Outcome {
    category: String,
    failure: Option<Failure>,
}

pub fn run(options: RunOptions) -> Result<RunResult, DatasetError> {
    if options.k == 0 {
        return Err(DatasetError::new("k must be positive"));
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
    let mut engine = options.engine;
    engine
        .prepare(&dataset.collection())
        .map_err(|error| DatasetError::new(format!("{} prepare failed: {error}", engine.name())))?;
    let engine: Arc<dyn Engine + Send + Sync> = Arc::from(engine);
    let collection = dataset.collection();
    let outcomes: Vec<Outcome> = cases
        .par_iter()
        .zip(expected.par_iter())
        .map(|(case, expected_ids)| {
            let failure = match engine.search(&case.query, options.k) {
                Ok(hits) => match parse_ids(&hits) {
                    Ok(actual_ids) => {
                        let comparison =
                            compare::compare(&case.query, &collection, expected_ids, &actual_ids);
                        build_failure(case, expected_ids, &actual_ids, comparison, None)
                    }
                    Err(message) => build_failure(
                        case,
                        expected_ids,
                        &[],
                        Comparison::default(),
                        Some(message),
                    ),
                },
                Err(error) => build_failure(
                    case,
                    expected_ids,
                    &[],
                    Comparison::default(),
                    Some(error.message),
                ),
            };
            Outcome {
                category: case.category.clone(),
                failure,
            }
        })
        .collect();
    let mut failures: Vec<Failure> = outcomes
        .iter()
        .filter_map(|outcome| outcome.failure.clone())
        .collect();
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
        if outcome.failure.is_some() {
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
    let missing = failures
        .iter()
        .map(|failure| failure.missing_ids.len())
        .sum();
    let extra = failures.iter().map(|failure| failure.extra_ids.len()).sum();
    let rank_swaps = failures
        .iter()
        .map(|failure| failure.rank_swaps.len())
        .sum();
    let errors = failures
        .iter()
        .filter(|failure| failure.error.is_some())
        .count();
    let cases_count = outcomes.len();
    let failed = failures.len();
    let summary = Summary {
        cases: cases_count,
        passed: cases_count - failed,
        failed,
        errors,
        missing,
        extra,
        rank_swaps,
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
            config_redacted: BTreeMap::new(),
        },
        k: options.k,
        summary,
        categories,
        failures,
    };
    let summary_line = format!(
        "{name} {version}: {failed}/{cases_count} cases mismatch ({missing} missing, {extra} extra, {rank_swaps} rank swaps)"
    );
    Ok(RunResult {
        report,
        summary_line,
    })
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
    })
}
