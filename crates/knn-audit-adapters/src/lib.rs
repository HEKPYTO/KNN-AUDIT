pub mod exact;
pub mod faiss;
pub mod pgvector;
pub mod qdrant;
pub mod reference;

pub use crate::faiss::FaissEngine;
pub use crate::pgvector::PgVectorEngine;
pub use crate::qdrant::QdrantEngine;
pub use crate::reference::ReferenceEngine;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Metric {
    SquaredL2,
    L2,
}

impl Metric {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SquaredL2 => "squared_l2",
            Self::L2 => "l2",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    pub distance: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub exact_mode: bool,
    pub distance_metric: Metric,
    pub supports_ids: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CollectionView<'a> {
    pub dim: usize,
    pub ids: &'a [u64],
    pub vectors: &'a [f32],
}

impl CollectionView<'_> {
    pub fn vector(&self, index: usize) -> &[f32] {
        &self.vectors[index * self.dim..(index + 1) * self.dim]
    }
}

#[derive(Debug)]
pub struct AdapterError {
    pub message: String,
}

impl AdapterError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AdapterError {}

pub trait Engine {
    fn name(&self) -> &'static str;
    fn version(&self) -> String;

    fn config(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError>;
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError>;
    fn capabilities(&self) -> Caps;
    fn reset(&mut self) -> Result<(), AdapterError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeEngine {
        ids: Vec<String>,
        expected_count: usize,
        exact: bool,
        prepared: bool,
    }

    impl FakeEngine {
        fn new(ids: &[&str], expected_count: usize, exact: bool) -> Self {
            Self {
                ids: ids.iter().map(|id| (*id).to_string()).collect(),
                expected_count,
                exact,
                prepared: false,
            }
        }
    }

    impl Engine for FakeEngine {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn version(&self) -> String {
            "fake-1".to_string()
        }

        fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
            if collection.ids.len() != self.expected_count {
                return Err(AdapterError::new(format!(
                    "count mismatch: engine reports {}, collection has {}",
                    self.expected_count,
                    collection.ids.len()
                )));
            }
            self.prepared = true;
            Ok(())
        }

        fn search(&self, query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError> {
            if !self.prepared {
                return Err(AdapterError::new("engine is not prepared"));
            }
            if !self.exact {
                return Err(AdapterError::new("engine cannot guarantee exact mode"));
            }
            if query.is_empty() {
                return Err(AdapterError::new("empty query"));
            }
            Ok(self
                .ids
                .iter()
                .take(k)
                .map(|id| Hit {
                    id: id.clone(),
                    distance: Some(1.0),
                })
                .collect())
        }

        fn capabilities(&self) -> Caps {
            Caps {
                exact_mode: self.exact,
                distance_metric: Metric::SquaredL2,
                supports_ids: true,
            }
        }
    }

    #[test]
    fn fake_prepare_is_idempotent_and_replaces_collection() {
        let view = CollectionView {
            dim: 2,
            ids: &[1, 2],
            vectors: &[0.0, 0.0, 1.0, 1.0],
        };
        let mut engine = FakeEngine::new(&["a", "b"], 2, true);
        engine.prepare(&view).expect("first prepare");
        engine.prepare(&view).expect("second prepare");
        assert_eq!(
            engine.search(&[0.0, 0.0], 2).expect("search"),
            vec![
                Hit {
                    id: "a".into(),
                    distance: Some(1.0)
                },
                Hit {
                    id: "b".into(),
                    distance: Some(1.0)
                }
            ]
        );
    }

    #[test]
    fn fake_reports_collection_count_mismatch() {
        let view = CollectionView {
            dim: 2,
            ids: &[1, 2],
            vectors: &[0.0, 0.0, 1.0, 1.0],
        };
        let mut engine = FakeEngine::new(&["a"], 3, true);
        let error = engine.prepare(&view).expect_err("count mismatch");
        assert!(error.message.contains("count mismatch"));
    }

    #[test]
    fn fake_caps_at_k_larger_than_collection() {
        let view = CollectionView {
            dim: 2,
            ids: &[7],
            vectors: &[0.0, 0.0],
        };
        let mut engine = FakeEngine::new(&["only"], 1, true);
        engine.prepare(&view).expect("prepare");
        assert_eq!(engine.search(&[0.0, 0.0], 10).expect("search").len(), 1);
        assert!(engine.capabilities().exact_mode);
        assert_eq!(engine.capabilities().distance_metric, Metric::SquaredL2);
    }

    #[test]
    fn fake_errors_without_exact_mode() {
        let view = CollectionView {
            dim: 1,
            ids: &[1],
            vectors: &[0.0],
        };
        let mut engine = FakeEngine::new(&["a"], 1, false);
        engine.prepare(&view).expect("prepare");
        assert!(engine.search(&[0.0], 1).is_err());
    }

    #[test]
    fn fake_preserves_unicode_id_strings_and_engine_order() {
        let ids: Vec<String> =
            serde_json::from_str(include_str!("../../../fixtures/unicode-ids/ids.json"))
                .expect("unicode ids fixture");
        let numeric_ids: Vec<u64> = (0..ids.len() as u64).collect();
        let vectors = vec![0.0; ids.len()];
        let view = CollectionView {
            dim: 1,
            ids: &numeric_ids,
            vectors: &vectors,
        };
        let mut engine = FakeEngine::new(
            &ids.iter().map(String::as_str).collect::<Vec<_>>(),
            ids.len(),
            true,
        );
        engine.prepare(&view).expect("prepare");
        let hits = engine.search(&[0.0], ids.len()).expect("search");
        assert_eq!(
            hits.into_iter().map(|hit| hit.id).collect::<Vec<_>>(),
            ids,
            "ids must round-trip as strings without re-sorting"
        );
    }
}
