use knn_audit::adapters::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};
use knn_audit::compare::Severity;
use knn_audit::dataset::Dataset;
use knn_audit::run::{RunOptions, run};
use std::sync::Mutex;

const MINI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mini");
const CORRECT: [u64; 10] = [10, 11, 9, 12, 8, 13, 7, 14, 6, 15];

struct EpochEngine {
    epoch: Mutex<usize>,
    answers: Vec<Vec<u64>>,
}

impl EpochEngine {
    fn new(answers: Vec<Vec<u64>>) -> Self {
        Self {
            epoch: Mutex::new(0),
            answers,
        }
    }
}

impl Engine for EpochEngine {
    fn name(&self) -> &'static str {
        "epoch"
    }

    fn version(&self) -> String {
        "test".to_string()
    }

    fn prepare(&mut self, _collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        let mut epoch = self.epoch.lock().expect("epoch counter");
        *epoch += 1;
        Ok(())
    }

    fn search(&self, _query: &[f32], _k: usize) -> Result<Vec<Hit>, AdapterError> {
        let epoch = *self.epoch.lock().expect("epoch counter");
        let ids = &self.answers[(epoch - 1).min(self.answers.len() - 1)];
        Ok(ids
            .iter()
            .map(|id| Hit {
                id: id.to_string(),
                distance: None,
            })
            .collect())
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::SquaredL2,
            supports_ids: true,
        }
    }
}

struct ErrorEngine;

impl Engine for ErrorEngine {
    fn name(&self) -> &'static str {
        "error"
    }

    fn version(&self) -> String {
        "test".to_string()
    }

    fn prepare(&mut self, _collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        Ok(())
    }

    fn search(&self, _query: &[f32], _k: usize) -> Result<Vec<Hit>, AdapterError> {
        Err(AdapterError::new("engine failed"))
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::SquaredL2,
            supports_ids: true,
        }
    }
}

struct BadIdEngine;

impl Engine for BadIdEngine {
    fn name(&self) -> &'static str {
        "bad-id"
    }

    fn version(&self) -> String {
        "test".to_string()
    }

    fn prepare(&mut self, _collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        Ok(())
    }

    fn search(&self, _query: &[f32], _k: usize) -> Result<Vec<Hit>, AdapterError> {
        Ok(vec![Hit {
            id: "nope".to_string(),
            distance: None,
        }])
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::SquaredL2,
            supports_ids: true,
        }
    }
}

struct SecondPrepareEngine {
    preparations: Mutex<usize>,
}

impl SecondPrepareEngine {
    fn new() -> Self {
        Self {
            preparations: Mutex::new(0),
        }
    }
}

impl Engine for SecondPrepareEngine {
    fn name(&self) -> &'static str {
        "second-prepare"
    }

    fn version(&self) -> String {
        "test".to_string()
    }

    fn prepare(&mut self, _collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        let mut count = self.preparations.lock().expect("prepare counter");
        *count += 1;
        if *count == 2 {
            Err(AdapterError::new("prepare broken"))
        } else {
            Ok(())
        }
    }

    fn search(&self, _query: &[f32], _k: usize) -> Result<Vec<Hit>, AdapterError> {
        Ok(CORRECT
            .iter()
            .map(|id| Hit {
                id: id.to_string(),
                distance: None,
            })
            .collect())
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::SquaredL2,
            supports_ids: true,
        }
    }
}

fn audit(answers: Vec<Vec<u64>>, repeat: usize) -> knn_audit::report::Report {
    audit_with_jobs(answers, repeat, None)
}

fn audit_with_jobs(
    answers: Vec<Vec<u64>>,
    repeat: usize,
    jobs: Option<usize>,
) -> knn_audit::report::Report {
    let dataset = Dataset::load(MINI, None).expect("mini fixture");
    run(RunOptions {
        dataset,
        engine: Box::new(EpochEngine::new(answers)),
        k: 10,
        limit: Some(1),
        repeat,
        jobs,
    })
    .expect("run")
    .report
}

#[test]
fn independent_audits_report_any_all_and_unstable() {
    let report = audit(
        vec![
            vec![90_001, 90_002, 90_003],
            vec![90_003, 90_002, 90_001],
            vec![90_001, 90_002, 90_004],
        ],
        3,
    );
    assert_eq!(report.repeat, 3);
    assert_eq!(report.summary.cases, 1);
    assert_eq!(report.summary.failed, 1);
    assert_eq!(report.summary.order_only, 0);
    assert_eq!(report.summary.failed_all, 1);
    assert_eq!(report.summary.unstable, 1);
    assert_eq!(report.runs.len(), 3);
    assert!(
        report
            .runs
            .iter()
            .all(|item| item.failed == 1 && item.missing > 0 && item.extra > 0)
    );
    let failure = &report.failures[0];
    assert_eq!(failure.severity, Severity::Mismatch);
    assert_eq!(failure.failed_repetitions, vec![1, 2, 3]);
    assert!(failure.unstable);
    assert_eq!(failure.unstable_actual_ids.len(), 2);
    let mut first = failure.unstable_actual_ids[0].clone();
    let mut second = failure.unstable_actual_ids[1].clone();
    let mut same_as_first = vec![90_003, 90_002, 90_001];
    first.sort_unstable();
    second.sort_unstable();
    same_as_first.sort_unstable();
    assert_eq!(first, same_as_first);
    assert_ne!(first, second);
}

#[test]
fn rank_swap_only_is_order_only_and_not_unstable() {
    let report = audit(
        vec![
            vec![11, 10, 9, 12, 8, 13, 7, 14, 6, 15],
            vec![10, 11, 12, 9, 8, 13, 7, 14, 6, 15],
        ],
        2,
    );
    assert_eq!(report.summary.failed, 0);
    assert_eq!(report.summary.order_only, 1);
    assert_eq!(report.summary.passed, 0);
    assert_eq!(report.summary.failed_all, 0);
    assert_eq!(report.summary.unstable, 0);
    assert!(
        report
            .runs
            .iter()
            .all(|item| item.order_only == 1 && item.failed == 0 && item.missing == 0)
    );
    let failure = &report.failures[0];
    assert_eq!(failure.severity, Severity::OrderOnly);
    assert!(!failure.unstable);
    assert!(failure.unstable_actual_ids.is_empty());
    assert!(failure.failed_repetitions.is_empty());
}

#[test]
fn a_failure_in_one_repetition_is_not_failed_all() {
    let report = audit(vec![CORRECT.to_vec(), vec![90_001]], 2);
    assert_eq!(report.summary.failed, 1);
    assert_eq!(report.summary.failed_all, 0);
    assert_eq!(report.summary.unstable, 1);
    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.failures[0].failed_repetitions, vec![2]);
    let missing = report.summary.missing;
    let extra = report.summary.extra;
    assert!(missing > 0 && extra > 0);
}

#[test]
fn single_audit_keeps_existing_counts() {
    let report = audit(vec![vec![90_001]], 1);
    assert_eq!(report.repeat, 1);
    assert_eq!(report.summary.failed, 1);
    assert_eq!(report.summary.failed_all, 1);
    assert_eq!(report.summary.unstable, 0);
    assert_eq!(report.runs.len(), 1);
    assert_eq!(report.failures[0].failed_repetitions, vec![1]);
    assert!(report.failures[0].unstable_actual_ids.is_empty());
}

#[test]
fn prepare_failure_in_second_repetition_aborts_the_run() {
    let dataset = Dataset::load(MINI, None).expect("mini fixture");
    let error = match run(RunOptions {
        dataset,
        engine: Box::new(SecondPrepareEngine::new()),
        k: 10,
        limit: Some(1),
        repeat: 2,
        jobs: Some(2),
    }) {
        Ok(_) => panic!("prepare failure"),
        Err(error) => error,
    };
    assert!(error.message.contains("second-prepare prepare failed"));
    assert!(error.message.contains("prepare broken"));
}

#[test]
fn local_jobs_do_not_change_the_report() {
    let answers = vec![vec![90_001], vec![90_001]];
    let one = audit_with_jobs(answers.clone(), 2, Some(1));
    let two = audit_with_jobs(answers, 2, Some(2));
    let mut one = serde_json::to_value(&one).expect("one report");
    let mut two = serde_json::to_value(&two).expect("two report");
    one.as_object_mut().expect("object").remove("created_at");
    two.as_object_mut().expect("object").remove("created_at");
    assert_eq!(one, two);
}

#[test]
fn adapter_error_counts_every_expected_id_as_missing() {
    let dataset = Dataset::load(MINI, None).expect("mini fixture");
    let result = run(RunOptions {
        dataset,
        engine: Box::new(ErrorEngine),
        k: 10,
        limit: Some(1),
        repeat: 1,
        jobs: None,
    })
    .expect("run");
    assert_eq!(result.report.summary.failed, 1);
    assert_eq!(result.report.summary.errors, 1);
    assert_eq!(result.report.summary.missing, CORRECT.len());
    assert_eq!(result.report.summary.extra, 0);
    assert_eq!(result.report.runs.len(), 1);
    assert_eq!(result.report.runs[0].failed, 1);
    assert_eq!(result.report.runs[0].errors, 1);
    assert_eq!(result.report.runs[0].missing, CORRECT.len());
    assert_eq!(result.report.failures[0].severity, Severity::Error);
    assert_eq!(
        result.report.failures[0].error.as_deref(),
        Some("engine failed")
    );
    assert!(result.summary_line.contains("1 errors; 10 missing"));
    assert!(
        result
            .report
            .to_html()
            .contains("<th>run</th><th>blocked</th>")
    );
}

#[test]
fn non_numeric_id_counts_every_expected_id_as_missing() {
    let dataset = Dataset::load(MINI, None).expect("mini fixture");
    let result = run(RunOptions {
        dataset,
        engine: Box::new(BadIdEngine),
        k: 10,
        limit: Some(1),
        repeat: 1,
        jobs: None,
    })
    .expect("run");
    assert_eq!(result.report.summary.errors, 1);
    assert_eq!(result.report.summary.missing, CORRECT.len());
    assert_eq!(result.report.failures[0].severity, Severity::Error);
    assert_eq!(result.report.failures[0].missing_ids.len(), CORRECT.len());
    assert!(
        result.report.failures[0]
            .error
            .as_deref()
            .expect("error")
            .contains("non-numeric id")
    );
}
