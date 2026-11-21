use crate::exact::top_k_exact;
use crate::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};

struct Collection {
    dim: usize,
    ids: Vec<u64>,
    vectors: Vec<f32>,
}

impl Collection {
    fn view(&self) -> CollectionView<'_> {
        CollectionView {
            dim: self.dim,
            ids: &self.ids,
            vectors: &self.vectors,
        }
    }
}

#[derive(Default)]
pub struct ReferenceEngine {
    collection: Option<Collection>,
}

impl ReferenceEngine {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Engine for ReferenceEngine {
    fn name(&self) -> &'static str {
        "reference"
    }

    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        if collection.dim == 0 {
            return Err(AdapterError::new("collection dimension must be positive"));
        }
        if collection.vectors.len() != collection.ids.len() * collection.dim {
            return Err(AdapterError::new(format!(
                "collection has {} values, expected {} for {} vectors of dimension {}",
                collection.vectors.len(),
                collection.ids.len() * collection.dim,
                collection.ids.len(),
                collection.dim
            )));
        }
        self.collection = Some(Collection {
            dim: collection.dim,
            ids: collection.ids.to_vec(),
            vectors: collection.vectors.to_vec(),
        });
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError> {
        let collection = self
            .collection
            .as_ref()
            .ok_or_else(|| AdapterError::new("reference engine is not prepared"))?;
        if query.len() != collection.dim {
            return Err(AdapterError::new(format!(
                "query dimension {} does not match collection dimension {}",
                query.len(),
                collection.dim
            )));
        }
        let ids = top_k_exact(query, &collection.view(), k);
        Ok(ids
            .into_iter()
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

    fn reset(&mut self) -> Result<(), AdapterError> {
        self.collection = None;
        Ok(())
    }
}
