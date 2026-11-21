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
use std::fmt::Write as _;

pub(crate) const BATCH_SIZE: usize = 1000;

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
}

pub(crate) fn validate_collection(collection: &CollectionView<'_>) -> Result<(), AdapterError> {
    if collection.dim == 0 || collection.vectors.len() != collection.ids.len() * collection.dim {
        return Err(AdapterError::new(format!(
            "count mismatch: collection has {} values, expected {} for {} vectors of dimension {}",
            collection.vectors.len(),
            collection.ids.len() * collection.dim,
            collection.ids.len(),
            collection.dim
        )));
    }
    Ok(())
}

pub(crate) fn verify_count(actual: u64, expected: usize) -> Result<(), AdapterError> {
    if actual as usize != expected {
        return Err(AdapterError::new(format!(
            "count mismatch: engine reports {actual}, collection has {expected}"
        )));
    }
    Ok(())
}

pub(crate) fn batches(ids: &[u64]) -> impl Iterator<Item = (usize, &[u64])> {
    ids.chunks(BATCH_SIZE)
        .enumerate()
        .map(|(batch, chunk)| (batch * BATCH_SIZE, chunk))
}

pub(crate) fn id_string(id: u64) -> String {
    id.to_string()
}

pub(crate) fn vector_text(values: &[f32]) -> String {
    let mut text = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        write!(&mut text, "{value}").expect("write to string");
    }
    text.push(']');
    text
}

pub(crate) fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 63
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_collection_accepts_matching_vectors() {
        let view = CollectionView {
            dim: 2,
            ids: &[1, 2],
            vectors: &[0.0, 0.0, 1.0, 1.0],
        };
        validate_collection(&view).expect("valid collection");
    }

    #[test]
    fn validate_collection_rejects_zero_dimension_and_short_vectors() {
        let zero = CollectionView {
            dim: 0,
            ids: &[1],
            vectors: &[],
        };
        let error = validate_collection(&zero).expect_err("zero dimension");
        assert!(error.message.contains("count mismatch"));
        let short = CollectionView {
            dim: 2,
            ids: &[1, 2],
            vectors: &[0.0, 0.0, 1.0],
        };
        let error = validate_collection(&short).expect_err("short vectors");
        assert!(error.message.contains("expected 4"));
        assert!(error.message.contains("count mismatch"));
    }

    #[test]
    fn verify_count_accepts_match_and_reports_mismatch() {
        verify_count(3, 3).expect("match");
        let error = verify_count(2, 3).expect_err("mismatch");
        assert_eq!(
            error.message,
            "count mismatch: engine reports 2, collection has 3"
        );
    }

    #[test]
    fn batches_returns_absolute_starts_and_slices() {
        let ids: Vec<u64> = (0..(BATCH_SIZE + 3) as u64).collect();
        let parts: Vec<_> = batches(&ids)
            .map(|(start, chunk)| (start, chunk.len(), chunk[0]))
            .collect();
        assert_eq!(
            parts,
            vec![(0, BATCH_SIZE, 0), (BATCH_SIZE, 3, BATCH_SIZE as u64)]
        );
    }

    #[test]
    fn id_string_and_vector_text_use_expected_text_format() {
        assert_eq!(id_string(7), "7");
        assert_eq!(vector_text(&[1.0, -2.5]), "[1,-2.5]");
    }

    #[test]
    fn valid_name_enforces_one_shared_grammar() {
        assert!(valid_name("audit_1"));
        assert!(valid_name("_"));
        assert!(!valid_name("1audit"));
        assert!(!valid_name("audit-1"));
        assert!(!valid_name(""));
        assert!(!valid_name(&"a".repeat(64)));
    }
}
