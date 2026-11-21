use crate::{
    AdapterError, Caps, CollectionView, Engine, Hit, Metric, batches, id_string, valid_name,
    validate_collection, verify_count,
};
use reqwest::blocking::Client;
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

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
            .map_err(|error| {
                AdapterError::new(format!("Qdrant client failed: {}", error.without_url()))
            })?;
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
            let message = status.get("error").unwrap_or(status).to_string();
            return Err(AdapterError::new(format!("qdrant status error: {message}")));
        }
        Ok(())
    }

    fn send(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value, AdapterError> {
        self.send_allow_missing(method, path, body, false)
    }

    fn send_allow_missing(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        allow_missing: bool,
    ) -> Result<Value, AdapterError> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().map_err(|error| {
            AdapterError::new(format!("qdrant request failed: {}", error.without_url()))
        })?;
        let status = response.status();
        if allow_missing && status == StatusCode::NOT_FOUND {
            return Ok(Value::Null);
        }
        let payload: Value = response.json().map_err(|error| {
            AdapterError::new(format!(
                "qdrant response is not JSON: {}",
                error.without_url()
            ))
        })?;
        if !status.is_success() {
            return Err(AdapterError::new(format!(
                "qdrant HTTP {status}: {payload}"
            )));
        }
        Self::check_status(&payload)?;
        Ok(payload)
    }

    pub fn cleanup(&mut self) -> Result<(), AdapterError> {
        self.send_allow_missing(Method::DELETE, &self.collection_path(), None, true)?;
        self.dim = None;
        self.version = None;
        Ok(())
    }
}

fn hnsw_is_flat(info: &Value) -> bool {
    info.pointer("/result/config/hnsw_config/m")
        .and_then(Value::as_u64)
        == Some(0)
}

impl Engine for QdrantEngine {
    fn name(&self) -> &'static str {
        "qdrant"
    }

    fn version(&self) -> String {
        self.version.clone().unwrap_or_default()
    }

    fn config(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("search".to_string(), "exact".to_string()),
            ("distance".to_string(), "euclid".to_string()),
            ("hnsw_m".to_string(), "0".to_string()),
            ("indexing_threshold".to_string(), "0".to_string()),
            ("default_segment_number".to_string(), "1".to_string()),
        ])
    }

    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        validate_collection(collection)?;
        self.dim = None;
        self.version = None;
        self.cleanup()?;
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
        if !hnsw_is_flat(&info) {
            return Err(AdapterError::new(format!(
                "qdrant config mismatch: hnsw m={:?}",
                info.pointer("/result/config/hnsw_config/m")
            )));
        }
        let version = self
            .send(Method::GET, "/", None)?
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AdapterError::new("qdrant response has no version"))?;

        for (start, chunk) in batches(collection.ids) {
            let points: Vec<Point> = chunk
                .iter()
                .enumerate()
                .map(|(offset, id)| Point {
                    id: *id,
                    vector: collection.vector(start + offset).to_vec(),
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
        verify_count(count, collection.ids.len())?;
        self.version = Some(version);
        self.dim = Some(collection.dim);
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
                    id: id_string(id),
                    distance: Some(distance),
                })
            })
            .collect()
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::L2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hnsw_is_flat;
    use serde_json::json;

    #[test]
    fn hnsw_is_flat_checks_the_server_reported_m() {
        assert!(hnsw_is_flat(
            &json!({"result": {"config": {"hnsw_config": {"m": 0}}}})
        ));
        assert!(!hnsw_is_flat(
            &json!({"result": {"config": {"hnsw_config": {"m": 16}}}})
        ));
        assert!(!hnsw_is_flat(&json!({"result": {"config": {}}})));
    }
}
