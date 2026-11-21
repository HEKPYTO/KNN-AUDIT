use knn_audit_adapters::CollectionView;
use knn_audit_adapters::exact::top_k_exact;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const EXPECTED_BASE_SHA256: &str =
    "c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb";

#[derive(Debug)]
pub struct DatasetError {
    pub message: String,
}

impl DatasetError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DatasetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DatasetError {}

#[derive(Debug, Clone)]
pub struct Case {
    pub case_id: String,
    pub category: String,
    pub query_row: u64,
    pub query: Vec<f32>,
    pub gt_ids: Vec<u64>,
}

#[derive(Debug)]
pub struct Dataset {
    pub root: String,
    pub base_sha256: String,
    pub base_rows: usize,
    pub dim: usize,
    pub vectors: Vec<f32>,
    pub ids: Vec<u64>,
    pub cases: Vec<Case>,
}

#[derive(Debug)]
pub struct VerifySummary {
    pub cases: usize,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CaseJson {
    case_id: String,
    category: String,
    query_row: u64,
    query: Vec<f32>,
    gt_ids: Vec<u64>,
}

impl Dataset {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, DatasetError> {
        let root = root.as_ref();
        let data_dir = root.join("data");
        let base_path = data_dir.join("base.csv");
        let bytes = fs::read(&base_path).map_err(|error| {
            DatasetError::new(format!("cannot read {}: {error}", base_path.display()))
        })?;
        let base_sha256 = digest_hex(&bytes);
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| DatasetError::new(format!("base.csv is not UTF-8: {error}")))?;
        let mut vectors = Vec::new();
        let mut dim = 0_usize;
        for (row, line) in text.lines().enumerate() {
            let mut columns = 0_usize;
            for field in line.split(',') {
                let value: f32 = field.trim().parse().map_err(|error| {
                    DatasetError::new(format!(
                        "base.csv row {row}, column {columns}: invalid fp32 value {field:?}: {error}"
                    ))
                })?;
                if !value.is_finite() {
                    return Err(DatasetError::new(format!(
                        "base.csv row {row}, column {columns}: non-finite value {field:?}"
                    )));
                }
                vectors.push(value);
                columns += 1;
            }
            if row == 0 {
                dim = columns;
                if dim == 0 {
                    return Err(DatasetError::new("base.csv is empty"));
                }
            } else if columns != dim {
                return Err(DatasetError::new(format!(
                    "base.csv row {row} has {columns} columns, expected {dim}"
                )));
            }
        }
        if vectors.is_empty() {
            return Err(DatasetError::new("base.csv contains no vectors"));
        }
        let base_rows = vectors.len() / dim;
        let mut case_paths = Vec::new();
        collect_json(&data_dir, &mut case_paths)?;
        case_paths.sort();
        if case_paths.is_empty() {
            return Err(DatasetError::new(format!(
                "no case files found under {}",
                data_dir.display()
            )));
        }
        let mut cases = Vec::with_capacity(case_paths.len());
        for path in case_paths {
            let bytes = fs::read(&path).map_err(|error| {
                DatasetError::new(format!("cannot read {}: {error}", path.display()))
            })?;
            let raw: CaseJson = serde_json::from_slice(&bytes).map_err(|error| {
                DatasetError::new(format!("malformed case {}: {error}", path.display()))
            })?;
            if raw.query.len() != dim {
                return Err(DatasetError::new(format!(
                    "case {} query has {} values, expected {dim}",
                    raw.case_id,
                    raw.query.len()
                )));
            }
            if raw.query.iter().any(|value| !value.is_finite()) {
                return Err(DatasetError::new(format!(
                    "case {} query has a non-finite value",
                    raw.case_id
                )));
            }
            if raw.query_row as usize >= base_rows {
                return Err(DatasetError::new(format!(
                    "case {} query_row {} is out of range for {base_rows} rows",
                    raw.case_id, raw.query_row
                )));
            }
            if raw.gt_ids.is_empty() {
                return Err(DatasetError::new(format!(
                    "case {} has no ground-truth ids",
                    raw.case_id
                )));
            }
            for id in &raw.gt_ids {
                if *id as usize >= base_rows {
                    return Err(DatasetError::new(format!(
                        "case {} ground-truth id {id} is out of range for {base_rows} rows",
                        raw.case_id
                    )));
                }
            }
            cases.push(Case {
                case_id: raw.case_id,
                category: raw.category,
                query_row: raw.query_row,
                query: raw.query,
                gt_ids: raw.gt_ids,
            });
        }
        let root_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dataset")
            .to_string();
        Ok(Self {
            root: root_name,
            base_sha256,
            base_rows,
            dim,
            vectors,
            ids: (0..base_rows as u64).collect(),
            cases,
        })
    }

    pub fn collection(&self) -> CollectionView<'_> {
        CollectionView {
            dim: self.dim,
            ids: &self.ids,
            vectors: &self.vectors,
        }
    }

    pub fn check_checksum(&self, expected: &str) -> Result<(), DatasetError> {
        if self.base_sha256.eq_ignore_ascii_case(expected) {
            Ok(())
        } else {
            Err(DatasetError::new(format!(
                "base.csv sha256 mismatch: expected {expected}, found {}",
                self.base_sha256
            )))
        }
    }

    pub fn is_shipped(&self) -> bool {
        self.base_rows == 22_501 && self.dim == 128 && self.cases.len() == 1_616
    }

    pub fn expected_ids(&self, case: &Case, k: usize) -> Result<Vec<u64>, DatasetError> {
        if k == 0 {
            return Err(DatasetError::new("k must be positive"));
        }
        if k > case.gt_ids.len() {
            return Err(DatasetError::new(format!(
                "case {} has {} ground-truth ids, k is {k}",
                case.case_id,
                case.gt_ids.len()
            )));
        }
        Ok(case.gt_ids[..k].to_vec())
    }

    pub fn verify(&self, indices: &[usize], k: usize) -> Result<VerifySummary, DatasetError> {
        let collection = self.collection();
        let mut mismatches = Vec::new();
        for &index in indices {
            let case = self.cases.get(index).ok_or_else(|| {
                DatasetError::new(format!(
                    "case index {index} is out of range for {} cases",
                    self.cases.len()
                ))
            })?;
            let mut expected = self.expected_ids(case, k)?;
            let mut actual = top_k_exact(&case.query, &collection, k);
            expected.sort_unstable();
            actual.sort_unstable();
            if expected != actual {
                mismatches.push(case.case_id.clone());
            }
        }
        Ok(VerifySummary {
            cases: indices.len(),
            mismatches,
        })
    }
}

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn collect_json(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), DatasetError> {
    let entries = fs::read_dir(dir)
        .map_err(|error| DatasetError::new(format!("cannot read {}: {error}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            DatasetError::new(format!("cannot enumerate {}: {error}", dir.display()))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_json(&path, paths)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
    Ok(())
}
