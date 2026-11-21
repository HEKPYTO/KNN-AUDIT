use knn_audit::adapters::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};
use knn_audit::dataset::Dataset;
use knn_audit::run::{RunOptions, run};
use std::sync::Mutex;

const MINI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mini");

#[derive(Default)]
struct FlakyEngine {
    calls: Mutex<usize>,
}

impl Engine for FlakyEngine {
    fn name(&self) -> &'static str {
        "flaky"
    }

    fn version(&self) -> String {
        "test".to_string()
    }

    fn prepare(&mut self, _collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        Ok(())
    }

    fn search(&self, _query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError> {
        let mut calls = self.calls.lock().expect("call counter");
        let base = if (*calls).is_multiple_of(2) {
            90_000
        } else {
            91_000
        };
        *calls += 1;
        Ok((0..k)
            .map(|offset| Hit {
                id: (base + offset as u64).to_string(),
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

#[test]
fn repeats_report_unstable_answers_and_keep_distinct_sets() {
    let dataset = Dataset::load(MINI).expect("mini fixture");
    let result = run(RunOptions {
        dataset,
        engine: Box::new(FlakyEngine::default()),
        k: 10,
        limit: Some(1),
        repeat: 2,
    })
    .expect("run");
    assert_eq!(result.report.summary.unstable, 1);
    assert_eq!(result.report.failures.len(), 1);
    let failure = &result.report.failures[0];
    assert!(failure.unstable);
    assert_eq!(failure.unstable_actual_ids.len(), 2);
    assert_eq!(failure.unstable_actual_ids[0][0], 90_000);
    assert_eq!(failure.unstable_actual_ids[1][0], 91_000);
}

#[test]
fn single_search_keeps_existing_report_shape() {
    let dataset = Dataset::load(MINI).expect("mini fixture");
    let result = run(RunOptions {
        dataset,
        engine: Box::new(FlakyEngine::default()),
        k: 10,
        limit: Some(1),
        repeat: 1,
    })
    .expect("run");
    assert_eq!(result.report.summary.unstable, 0);
    assert_eq!(result.report.failures.len(), 1);
    let failure = &result.report.failures[0];
    assert!(!failure.unstable);
    assert!(failure.unstable_actual_ids.is_empty());
    assert_eq!(failure.actual_ids[0], 90_000);
}
