use crate::compare::{self, Comparison, Severity};
use crate::dataset::{Case, Dataset, DatasetError};
use crate::report::{
    CategorySummary, DatasetInfo, EngineInfo, Failure, Report, RunSummary, Summary, ToolInfo,
};
use chrono::{SecondsFormat, Utc};
use knn_audit_adapters::{CollectionView, Engine, Hit};
use rayon::ThreadPool;
use rayon::prelude::*;
use std::collections::BTreeMap;

pub struct RunOptions {
    pub dataset: Dataset,
    pub engine: Box<dyn Engine + Send + Sync>,
    pub k: usize,
    pub limit: Option<usize>,
    pub repeat: usize,
    pub jobs: Option<usize>,
}

pub struct RunResult {
    pub report: Report,
    pub summary_line: String,
}

struct CaseVerdict {
    category: String,
    answers: Vec<Vec<u64>>,
    severities: Vec<Severity>,
    failures: Vec<Option<Failure>>,
    failed_repetitions: Vec<usize>,
}

impl CaseVerdict {
    fn observe(&mut self, severity: Severity, ids: Vec<u64>, failure: Option<Failure>) {
        self.answers.push(ids);
        self.severities.push(severity);
        self.failures.push(failure);
        if blocking(severity) {
            self.failed_repetitions.push(self.severities.len());
        }
    }

    fn first_failure(&self) -> Option<&Failure> {
        self.failures
            .iter()
            .flatten()
            .find(|failure| blocking(failure.severity))
            .or_else(|| self.failures.iter().flatten().next())
    }

    fn unstable_sets(&self) -> Vec<Vec<u64>> {
        let mut sets: Vec<&Vec<u64>> = Vec::new();
        for answer in &self.answers {
            if !sets.iter().any(|seen| same_set(seen, answer)) {
                sets.push(answer);
            }
        }
        sets.into_iter().cloned().collect()
    }
}

pub fn run(options: RunOptions) -> Result<RunResult, DatasetError> {
    if options.k == 0 {
        return Err(DatasetError::new("k must be positive"));
    }
    if options.repeat == 0 {
        return Err(DatasetError::new("repeat must be positive"));
    }
    let selected = select(&options.dataset, options.limit, options.k)?;
    let collection = options.dataset.collection();
    let pool = build_pool(options.jobs)?;
    let mut engine = options.engine;
    let name = engine.name();
    let mut verdicts: Vec<CaseVerdict> = selected
        .iter()
        .map(|(case, _)| CaseVerdict {
            category: case.category.clone(),
            answers: Vec::with_capacity(options.repeat),
            severities: Vec::with_capacity(options.repeat),
            failures: Vec::with_capacity(options.repeat),
            failed_repetitions: Vec::new(),
        })
        .collect();
    let mut runs = Vec::with_capacity(options.repeat);
    for _ in 0..options.repeat {
        engine
            .prepare(&collection)
            .map_err(|error| DatasetError::new(format!("{name} prepare failed: {error}")))?;
        let observed = search(&pool, engine.as_ref(), &selected, &collection, options.k);
        runs.push(summarize_run(&observed));
        for (verdict, (severity, ids, failure)) in verdicts.iter_mut().zip(observed) {
            verdict.observe(severity, ids, failure);
        }
    }

    let summary = summarize(&verdicts);
    let categories = summarize_categories(&verdicts);
    let failures = collect_failures(&verdicts);
    let report = Report {
        schema: crate::report::SCHEMA.to_string(),
        tool: ToolInfo {
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        created_at: Some(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)),
        dataset: DatasetInfo {
            root: Some(options.dataset.root.clone()),
            base_sha256: options.dataset.base_sha256.clone(),
            base_rows: options.dataset.base_rows,
            cases: options.dataset.cases.len(),
            dim: options.dataset.dim,
        },
        engine: EngineInfo {
            name: name.to_string(),
            version: engine.version(),
            exact_mode: engine.capabilities().exact_mode,
            metric: engine.capabilities().distance_metric.as_str().to_string(),
            config: engine.config(),
        },
        k: options.k,
        summary: summary.clone(),
        categories,
        failures,
        repeat: options.repeat,
        runs,
    };
    let summary_line = format!(
        "{name} {} (repeat {}): {}/{} cases blocked ({} in every run, {} order-only, {} unstable; {} errors; {} missing, {} extra, {} rank swaps)",
        engine.version(),
        options.repeat,
        summary.failed,
        summary.cases,
        summary.failed_all,
        summary.order_only,
        summary.unstable,
        summary.errors,
        summary.missing,
        summary.extra,
        summary.rank_swaps
    );
    Ok(RunResult {
        report,
        summary_line,
    })
}

fn select(
    dataset: &Dataset,
    limit: Option<usize>,
    k: usize,
) -> Result<Vec<(&Case, Vec<u64>)>, DatasetError> {
    dataset
        .cases
        .iter()
        .take(limit.unwrap_or(dataset.cases.len()))
        .map(|case| {
            let expected = dataset.expected_ids(case, k)?;
            Ok((case, expected))
        })
        .collect()
}

fn build_pool(jobs: Option<usize>) -> Result<ThreadPool, DatasetError> {
    let mut builder = rayon::ThreadPoolBuilder::new();
    if let Some(count) = jobs {
        if count == 0 {
            return Err(DatasetError::new("--jobs must be positive"));
        }
        builder = builder.num_threads(count);
    }
    builder
        .build()
        .map_err(|error| DatasetError::new(format!("cannot build thread pool: {error}")))
}

fn search<E: Engine + Sync + ?Sized>(
    pool: &ThreadPool,
    engine: &E,
    selected: &[(&Case, Vec<u64>)],
    collection: &CollectionView<'_>,
    k: usize,
) -> Vec<(Severity, Vec<u64>, Option<Failure>)> {
    pool.install(|| {
        selected
            .par_iter()
            .map(|(case, expected_ids)| {
                let (ids, comparison, error) = match engine.search(&case.query, k) {
                    Ok(hits) => match parse_ids(&hits) {
                        Ok(ids) => {
                            let comparison =
                                compare::compare(&case.query, collection, expected_ids, &ids);
                            (ids, comparison, None)
                        }
                        Err(message) => {
                            let comparison =
                                compare::compare(&case.query, collection, expected_ids, &[]);
                            (Vec::new(), comparison, Some(message))
                        }
                    },
                    Err(error) => {
                        let comparison =
                            compare::compare(&case.query, collection, expected_ids, &[]);
                        (Vec::new(), comparison, Some(error.message))
                    }
                };
                let severity = comparison.severity(error.is_some());
                let failure = build_failure(case, expected_ids, &ids, comparison, severity, error);
                (severity, ids, failure)
            })
            .collect()
    })
}

fn summarize_run(observed: &[(Severity, Vec<u64>, Option<Failure>)]) -> RunSummary {
    let mut summary = RunSummary {
        failed: 0,
        order_only: 0,
        errors: 0,
        missing: 0,
        extra: 0,
        rank_swaps: 0,
    };
    for (severity, _, failure) in observed {
        match severity {
            Severity::Error => {
                summary.failed += 1;
                summary.errors += 1;
            }
            Severity::Mismatch => summary.failed += 1,
            Severity::OrderOnly => summary.order_only += 1,
            Severity::Clean => {}
        }
        if let Some(failure) = failure {
            summary.missing += failure.missing_ids.len();
            summary.extra += failure.extra_ids.len();
            summary.rank_swaps += failure.rank_swaps.len();
        }
    }
    summary
}

fn summarize(verdicts: &[CaseVerdict]) -> Summary {
    let cases = verdicts.len();
    let failed = verdicts
        .iter()
        .filter(|verdict| {
            verdict
                .severities
                .iter()
                .any(|severity| blocking(*severity))
        })
        .count();
    let order_only = verdicts
        .iter()
        .filter(|verdict| {
            !verdict
                .severities
                .iter()
                .any(|severity| blocking(*severity))
                && verdict.severities.contains(&Severity::OrderOnly)
        })
        .count();
    let errors = verdicts
        .iter()
        .filter(|verdict| verdict.severities.contains(&Severity::Error))
        .count();
    let failed_all = verdicts
        .iter()
        .filter(|verdict| {
            verdict
                .severities
                .iter()
                .all(|severity| blocking(*severity))
        })
        .count();
    let unstable_sets: Vec<usize> = verdicts
        .iter()
        .map(|verdict| verdict.unstable_sets().len())
        .collect();
    let unstable = unstable_sets.iter().filter(|count| **count > 1).count();
    let first_failures: Vec<&Failure> = verdicts
        .iter()
        .filter_map(CaseVerdict::first_failure)
        .collect();
    Summary {
        cases,
        passed: cases - failed - order_only,
        failed,
        order_only,
        errors,
        missing: first_failures
            .iter()
            .map(|failure| failure.missing_ids.len())
            .sum(),
        extra: first_failures
            .iter()
            .map(|failure| failure.extra_ids.len())
            .sum(),
        rank_swaps: first_failures
            .iter()
            .map(|failure| failure.rank_swaps.len())
            .sum(),
        unstable,
        failed_all,
    }
}

fn summarize_categories(verdicts: &[CaseVerdict]) -> Vec<CategorySummary> {
    let mut counts: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for verdict in verdicts {
        let entry = counts.entry(verdict.category.clone()).or_insert((0, 0, 0));
        entry.0 += 1;
        if verdict
            .severities
            .iter()
            .any(|severity| blocking(*severity))
        {
            entry.1 += 1;
        } else if verdict.severities.contains(&Severity::OrderOnly) {
            entry.2 += 1;
        }
    }
    counts
        .into_iter()
        .map(|(category, (cases, failed, order_only))| CategorySummary {
            category,
            cases,
            failed,
            order_only,
        })
        .collect()
}

fn collect_failures(verdicts: &[CaseVerdict]) -> Vec<Failure> {
    let mut failures = Vec::new();
    for verdict in verdicts {
        let Some(first) = verdict.first_failure() else {
            continue;
        };
        let mut failure = first.clone();
        failure.failed_repetitions = verdict.failed_repetitions.clone();
        let sets = verdict.unstable_sets();
        if sets.len() > 1 {
            failure.unstable = true;
            failure.unstable_actual_ids = sets;
        }
        failures.push(failure);
    }
    failures.sort_by(|left, right| {
        left.case_id
            .cmp(&right.case_id)
            .then_with(|| left.category.cmp(&right.category))
    });
    failures
}

fn blocking(severity: Severity) -> bool {
    matches!(severity, Severity::Mismatch | Severity::Error)
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

fn parse_ids(hits: &[Hit]) -> Result<Vec<u64>, String> {
    hits.iter()
        .map(|hit| {
            hit.id
                .parse::<u64>()
                .map_err(|_| format!("engine returned non-numeric id {:?}", hit.id))
        })
        .collect()
}

fn build_failure(
    case: &Case,
    expected_ids: &[u64],
    actual_ids: &[u64],
    comparison: Comparison,
    severity: Severity,
    error: Option<String>,
) -> Option<Failure> {
    if severity == Severity::Clean {
        return None;
    }
    Some(Failure {
        case_id: case.case_id.clone(),
        category: case.category.clone(),
        query: Some(case.query.clone()),
        severity,
        expected_ids: expected_ids.to_vec(),
        actual_ids: actual_ids.to_vec(),
        missing_ids: comparison.missing_ids,
        extra_ids: comparison.extra_ids,
        rank_swaps: comparison.rank_swaps,
        distance_gap: comparison.distance_gap,
        error,
        failed_repetitions: Vec::new(),
        unstable: false,
        unstable_actual_ids: Vec::new(),
    })
}
