use crate::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};
use serde::Deserialize;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

const BATCH_SIZE: usize = 1000;

#[derive(Deserialize)]
struct SearchHit {
    id: u64,
    distance: f64,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Worker {
    fn request(&mut self, request: &Value) -> Result<Value, AdapterError> {
        let mut text = request.to_string();
        text.push('\n');
        self.stdin
            .write_all(text.as_bytes())
            .map_err(|error| AdapterError::new(format!("faiss worker write failed: {error}")))?;
        self.stdin
            .flush()
            .map_err(|error| AdapterError::new(format!("faiss worker flush failed: {error}")))?;
        let mut line = String::new();
        let size = self
            .stdout
            .read_line(&mut line)
            .map_err(|error| AdapterError::new(format!("faiss worker read failed: {error}")))?;
        if size == 0 {
            return Err(AdapterError::new("faiss worker closed unexpectedly"));
        }
        let response: Value = serde_json::from_str(&line).map_err(|error| {
            AdapterError::new(format!("faiss worker response is not JSON: {error}"))
        })?;
        if let Some(message) = response.get("error").and_then(Value::as_str) {
            return Err(AdapterError::new(message.to_string()));
        }
        Ok(response)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.request(&json!({ "op": "close" }));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct FaissEngine {
    worker: Mutex<Worker>,
    version: Option<String>,
    dim: Option<usize>,
}

impl FaissEngine {
    pub fn new(python: impl AsRef<OsStr>) -> Result<Self, AdapterError> {
        let mut child = Command::new(python)
            .arg("-c")
            .arg(include_str!("../scripts/faiss_worker.py"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                AdapterError::new(format!("faiss interpreter failed to start: {error}"))
            })?;
        let stdin = child.stdin.take().expect("piped faiss stdin");
        let stdout = child.stdout.take().expect("piped faiss stdout");
        Ok(Self {
            worker: Mutex::new(Worker {
                child,
                stdin,
                stdout: BufReader::new(stdout),
            }),
            version: None,
            dim: None,
        })
    }

    fn send(&self, request: &Value) -> Result<Value, AdapterError> {
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| AdapterError::new("faiss worker lock is poisoned"))?;
        worker.request(request)
    }
}

impl Engine for FaissEngine {
    fn name(&self) -> &'static str {
        "faiss"
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
        let version = self
            .send(&json!({ "op": "version" }))?
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AdapterError::new("faiss version response has no version"))?;
        self.send(&json!({ "op": "create", "dim": collection.dim }))?;
        for (batch, ids) in collection.ids.chunks(BATCH_SIZE).enumerate() {
            let start = batch * BATCH_SIZE;
            let mut vectors = Vec::with_capacity(ids.len() * collection.dim);
            for offset in 0..ids.len() {
                vectors.extend_from_slice(collection.vector(start + offset));
            }
            self.send(&json!({
                "op": "add",
                "ids": ids,
                "vectors": vectors,
            }))?;
        }
        let count = self
            .send(&json!({ "op": "count" }))?
            .get("count")
            .and_then(Value::as_u64)
            .ok_or_else(|| AdapterError::new("faiss count response has no count"))?;
        if count as usize != collection.ids.len() {
            return Err(AdapterError::new(format!(
                "count mismatch: engine reports {count}, collection has {}",
                collection.ids.len()
            )));
        }
        self.version = Some(version);
        self.dim = Some(collection.dim);
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError> {
        let dim = self
            .dim
            .ok_or_else(|| AdapterError::new("faiss is not prepared"))?;
        if query.len() != dim {
            return Err(AdapterError::new(format!(
                "query dimension {} does not match collection dimension {dim}",
                query.len()
            )));
        }
        if k == 0 {
            return Ok(Vec::new());
        }
        let response = self.send(&json!({
            "op": "search",
            "query": query,
            "k": k,
        }))?;
        let hits: Vec<SearchHit> = serde_json::from_value(response).map_err(|error| {
            AdapterError::new(format!("faiss search response is invalid: {error}"))
        })?;
        Ok(hits
            .into_iter()
            .map(|hit| Hit {
                id: hit.id.to_string(),
                distance: Some(hit.distance),
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
        self.send(&json!({ "op": "reset" }))?;
        self.dim = None;
        Ok(())
    }
}
