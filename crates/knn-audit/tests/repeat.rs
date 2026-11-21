use knn_audit::adapters::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};
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

fn audit(answers: Vec<Vec<u64>>, repeat: usize) -> knn_audit::report::Report {
    let dataset = Dataset::load(MINI).expect("mini fixture");
    run(RunOptions {
        dataset,
        engine: Box::new(EpochEngine::new(answers)),
        k: 10,
        limit: Some(1),
        repeat,
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
    assert_eq!(report.summary.failed_all, 1);
    assert_eq!(report.summary.unstable, 1);
    assert_eq!(report.runs.len(), 3);
    assert!(
        report.runs.iter().all(|item| item.failed == 1
            && item.missing > 0
            && item.extra > 0
            && item.unstable == 1)
    );
    let failure = &report.failures[0];
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
fn rank_swap_only_is_not_unstable() {
    let report = audit(
        vec![
            vec![11, 10, 9, 12, 8, 13, 7, 14, 6, 15],
            vec![10, 11, 12, 9, 8, 13, 7, 14, 6, 15],
        ],
        2,
    );
    assert_eq!(report.summary.failed_all, 1);
    assert_eq!(report.summary.unstable, 0);
    assert!(
        report
            .runs
            .iter()
            .all(|item| item.rank_swaps > 0 && item.missing == 0 && item.extra == 0)
    );
    let failure = &report.failures[0];
    assert!(!failure.unstable);
    assert!(failure.unstable_actual_ids.is_empty());
    assert_eq!(failure.failed_repetitions, vec![1, 2]);
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
fn adapter_error_counts_every_expected_id_as_missing() {
    let dataset = Dataset::load(MINI).expect("mini fixture");
    let result = run(RunOptions {
        dataset,
        engine: Box::new(ErrorEngine),
        k: 10,
        limit: Some(1),
        repeat: 1,
    })
    .expect("run");
    assert_eq!(result.report.summary.errors, 1);
    assert_eq!(result.report.summary.missing, CORRECT.len());
    assert_eq!(result.report.summary.extra, 0);
    assert_eq!(result.report.runs.len(), 1);
    assert_eq!(result.report.runs[0].errors, 1);
    assert_eq!(result.report.runs[0].missing, CORRECT.len());
    assert_eq!(
        result.report.failures[0].error.as_deref(),
        Some("engine failed")
    );
    assert!(result.summary_line.contains("1 errors; 10 missing"));
    assert!(
        result
            .report
            .to_html()
            .contains("<th>run</th><th>failed</th><th>errors</th>")
    );
}
