use crate::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};
use reqwest::blocking::Client;
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Duration;

const BATCH_SIZE: usize = 1000;

#[derive(Serialize)]
struct Point {
    id: u64,
    vector: Vec<f32>,
}

pub struct QdrantEngine {
    client: Client,
    base_url: String,
    collection: String,
    version: Option<String>,
    dim: Option<usize>,
}

impl QdrantEngine {
    pub fn new(
        url: impl Into<String>,
        collection: impl Into<String>,
    ) -> Result<Self, AdapterError> {
        let collection = collection.into();
        if !valid_name(&collection) {
            return Err(AdapterError::new("invalid Qdrant collection name"));
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|error| AdapterError::new(format!("Qdrant client failed: {error}")))?;
        Ok(Self {
            client,
            base_url: url.into().trim_end_matches('/').to_string(),
            collection,
            version: None,
            dim: None,
        })
    }

    fn collection_path(&self) -> String {
        format!("/collections/{}", self.collection)
    }

    fn check_status(payload: &Value) -> Result<(), AdapterError> {
        if let Some(status) = payload.get("status")
            && status != "ok"
        {
            let message = status
                .get("error")
                .map_or_else(|| status.to_string(), Value::to_string);
            return Err(AdapterError::new(format!("qdrant status error: {message}")));
        }
        Ok(())
    }

    fn send(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value, AdapterError> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .map_err(|error| AdapterError::new(format!("qdrant request failed: {error}")))?;
        let status = response.status();
        let payload: Value = response
            .json()
            .map_err(|error| AdapterError::new(format!("qdrant response is not JSON: {error}")))?;
        if !status.is_success() {
            return Err(AdapterError::new(format!(
                "qdrant HTTP {status}: {payload}"
            )));
        }
        Self::check_status(&payload)?;
        Ok(payload)
    }

    fn delete_collection(&self) -> Result<(), AdapterError> {
        let response = self
            .client
            .delete(format!("{}{}", self.base_url, self.collection_path()))
            .send()
            .map_err(|error| AdapterError::new(format!("qdrant request failed: {error}")))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let payload: Value = response
            .json()
            .map_err(|error| AdapterError::new(format!("qdrant response is not JSON: {error}")))?;
        if !status.is_success() {
            return Err(AdapterError::new(format!(
                "qdrant delete failed: {payload}"
            )));
        }
        Self::check_status(&payload)
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Engine for QdrantEngine {
    fn name(&self) -> &'static str {
        "qdrant"
    }

    fn version(&self) -> String {
        self.version.clone().unwrap_or_default()
    }

    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        if collection.dim == 0 || collection.vectors.len() != collection.ids.len() * collection.dim
        {
            return Err(AdapterError::new(format!(
                "count mismatch: collection has {} values, expected {} for {} vectors of dimension {}",
                collection.vectors.len(),
                collection.ids.len() * collection.dim,
                collection.ids.len(),
                collection.dim
            )));
        }
        self.delete_collection()?;
        self.send(
            Method::PUT,
            &self.collection_path(),
            Some(json!({
                "vectors": {
                    "size": collection.dim,
                    "distance": "Euclid",
                    "hnsw_config": { "m": 0 }
                },
                "hnsw_config": { "m": 0 },
                "optimizer_config": {
                    "indexing_threshold": 0,
                    "default_segment_number": 1,
                    "flush_interval_sec": 1
                }
            })),
        )?;
        let info = self.send(Method::GET, &self.collection_path(), None)?;
        let m = info
            .pointer("/result/config/hnsw_config/m")
            .and_then(Value::as_u64);
        let indexed = info
            .pointer("/result/indexed_vectors_count")
            .and_then(Value::as_u64);
        if m != Some(0) || indexed != Some(0) {
            return Err(AdapterError::new(format!(
                "qdrant config mismatch: hnsw m={m:?}, indexed vectors={indexed:?}"
            )));
        }
        let root = self.send(Method::GET, "/", None)?;
        self.version = Some(
            root.get("version")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| AdapterError::new("qdrant response has no version"))?,
        );
        self.dim = Some(collection.dim);

        for (batch, chunk) in collection.ids.chunks(BATCH_SIZE).enumerate() {
            let points: Vec<Point> = chunk
                .iter()
                .enumerate()
                .map(|(offset, id)| Point {
                    id: *id,
                    vector: collection.vector(batch * BATCH_SIZE + offset).to_vec(),
                })
                .collect();
            self.send(
                Method::PUT,
                &format!("{}/points?wait=true", self.collection_path()),
                Some(json!({ "points": points })),
            )?;
        }

        let count = self
            .send(
                Method::POST,
                &format!("{}/points/count", self.collection_path()),
                Some(json!({ "exact": true })),
            )?
            .pointer("/result/count")
            .and_then(Value::as_u64)
            .ok_or_else(|| AdapterError::new("qdrant count response has no count"))?;
        if count as usize != collection.ids.len() {
            return Err(AdapterError::new(format!(
                "count mismatch: engine reports {count}, collection has {}",
                collection.ids.len()
            )));
        }
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError> {
        let dim = self
            .dim
            .ok_or_else(|| AdapterError::new("qdrant is not prepared"))?;
        if query.len() != dim {
            return Err(AdapterError::new(format!(
                "query dimension {} does not match collection dimension {dim}",
                query.len()
            )));
        }
        if k == 0 {
            return Ok(Vec::new());
        }
        let payload = self.send(
            Method::POST,
            &format!("{}/points/query", self.collection_path()),
            Some(json!({
                "query": query,
                "limit": k,
                "params": { "exact": true },
                "with_payload": false
            })),
        )?;
        let points = payload
            .pointer("/result/points")
            .and_then(Value::as_array)
            .ok_or_else(|| AdapterError::new("qdrant query response has no points"))?;
        points
            .iter()
            .map(|point| {
                let id = point
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| AdapterError::new("qdrant point has no numeric id"))?;
                let distance = point
                    .get("score")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| AdapterError::new("qdrant point has no score"))?;
                Ok(Hit {
                    id: id.to_string(),
                    distance: Some(distance),
                })
            })
            .collect()
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::L2,
            supports_ids: true,
        }
    }

    fn reset(&mut self) -> Result<(), AdapterError> {
        self.delete_collection()?;
        self.dim = None;
        Ok(())
    }
}
