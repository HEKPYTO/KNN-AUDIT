use crate::{
    AdapterError, Caps, CollectionView, Engine, Hit, Metric, batches, id_string,
    validate_collection, verify_count,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Deserialize)]
struct SearchHit {
    id: u64,
    distance: f64,
}

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<String>,
    reader: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(mut child: Child) -> Self {
        let stdin = child.stdin.take().expect("piped faiss stdin");
        let stdout = child.stdout.take().expect("piped faiss stdout");
        let (sender, responses) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if sender.send(line.clone()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Self {
            child,
            stdin,
            responses,
            reader: Some(reader),
        }
    }

    fn terminate(&mut self, message: impl Into<String>) -> AdapterError {
        let _ = self.child.kill();
        let _ = self.child.wait();
        AdapterError::new(message)
    }

    fn request(&mut self, request: &Value) -> Result<Value, AdapterError> {
        let mut text = request.to_string();
        text.push('\n');
        let write = self
            .stdin
            .write_all(text.as_bytes())
            .and_then(|()| self.stdin.flush());
        if let Err(error) = write {
            return Err(self.terminate(format!("faiss worker write failed: {error}")));
        }
        let line = match self.responses.recv_timeout(REQUEST_TIMEOUT) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                return Err(self.terminate("faiss worker request timed out"));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(self.terminate("faiss worker closed unexpectedly"));
            }
        };
        let response: Value = match serde_json::from_str(&line) {
            Ok(response) => response,
            Err(error) => {
                return Err(self.terminate(format!("faiss worker response is not JSON: {error}")));
            }
        };
        if let Some(message) = response.get("error").and_then(Value::as_str) {
            return Err(AdapterError::new(message.to_string()));
        }
        Ok(response)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub struct FaissEngine {
    worker: Mutex<Worker>,
    version: Option<String>,
    dim: Option<usize>,
}

impl FaissEngine {
    pub fn new(python: impl AsRef<OsStr>) -> Result<Self, AdapterError> {
        let interpreter = python.as_ref().to_string_lossy().into_owned();
        let child = Command::new(python)
            .arg("-P")
            .arg("-c")
            .arg(include_str!("../scripts/faiss_worker.py"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                AdapterError::new(format!(
                    "faiss interpreter {interpreter} failed to start: {error}"
                ))
            })?;
        Ok(Self {
            worker: Mutex::new(Worker::new(child)),
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

    fn config(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("index".to_string(), "IndexFlatL2".to_string()),
            ("metric".to_string(), "squared_l2".to_string()),
            ("overflow".to_string(), "reported_as_error".to_string()),
            ("worker".to_string(), "serial".to_string()),
        ])
    }

    fn prepare(&mut self, collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        validate_collection(collection)?;
        self.dim = None;
        self.version = None;
        let version = self
            .send(&json!({ "op": "version" }))?
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AdapterError::new("faiss version response has no version"))?;
        self.send(&json!({ "op": "create", "dim": collection.dim }))?;
        for (start, ids) in batches(collection.ids) {
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
        verify_count(count, collection.ids.len())?;
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
        let response: SearchResponse = serde_json::from_value(response).map_err(|error| {
            AdapterError::new(format!("faiss search response is invalid: {error}"))
        })?;
        Ok(response
            .hits
            .into_iter()
            .map(|hit| Hit {
                id: id_string(hit.id),
                distance: Some(hit.distance),
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
