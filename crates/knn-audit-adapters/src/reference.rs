use crate::exact::top_k_exact;
use crate::{
    AdapterError, Caps, CollectionView, Engine, Hit, Metric, id_string, validate_collection,
};
use std::collections::BTreeMap;

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

    fn config(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("kernel".to_string(), "dyadic-rational".to_string()),
            ("screening".to_string(), "certified-f64".to_string()),
            ("tie_break".to_string(), "smaller-id".to_string()),
        ])
    }

    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        validate_collection(collection)?;
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
        let ids = top_k_exact(query, &collection.view(), k)?;
        Ok(ids
            .into_iter()
            .map(|id| Hit {
                id: id_string(id),
                distance: None,
            })
            .collect())
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::SquaredL2,
        }
    }
}
