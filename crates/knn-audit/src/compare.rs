use knn_audit_adapters::CollectionView;
use knn_audit_adapters::exact::squared_l2_f64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankSwap {
    pub id: u64,
    pub expected_rank: usize,
    pub actual_rank: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comparison {
    pub missing_ids: Vec<u64>,
    pub extra_ids: Vec<u64>,
    pub rank_swaps: Vec<RankSwap>,
    pub distance_gap: Option<f64>,
}

impl Comparison {
    pub fn is_set_mismatch(&self) -> bool {
        !self.missing_ids.is_empty() || !self.extra_ids.is_empty()
    }
}

pub fn compare(
    query: &[f32],
    collection: &CollectionView<'_>,
    expected: &[u64],
    actual: &[u64],
) -> Comparison {
    let mut expected_counts = BTreeMap::new();
    let mut actual_counts = BTreeMap::new();
    let mut expected_ranks = BTreeMap::new();
    let mut actual_ranks = BTreeMap::new();
    for (rank, id) in expected.iter().enumerate() {
        *expected_counts.entry(*id).or_insert(0_usize) += 1;
        expected_ranks.entry(*id).or_insert(rank + 1);
    }
    for (rank, id) in actual.iter().enumerate() {
        *actual_counts.entry(*id).or_insert(0_usize) += 1;
        actual_ranks.entry(*id).or_insert(rank + 1);
    }
    let mut missing_ids = Vec::new();
    let mut extra_ids = Vec::new();
    let mut rank_swaps = Vec::new();
    for (id, count) in &expected_counts {
        let found = actual_counts.get(id).copied().unwrap_or(0);
        for _ in found..*count {
            missing_ids.push(*id);
        }
        if let Some(actual_rank) = actual_ranks.get(id) {
            let expected_rank = expected_ranks[id];
            if expected_rank != *actual_rank {
                rank_swaps.push(RankSwap {
                    id: *id,
                    expected_rank,
                    actual_rank: *actual_rank,
                });
            }
        }
    }
    for (id, count) in &actual_counts {
        let wanted = expected_counts.get(id).copied().unwrap_or(0);
        for _ in wanted..*count {
            extra_ids.push(*id);
        }
    }
    let distance_gap = match (
        expected
            .last()
            .and_then(|id| vector_for_id(collection, *id)),
        actual.last().and_then(|id| vector_for_id(collection, *id)),
    ) {
        (Some(expected_vector), Some(actual_vector)) if query.len() == collection.dim => {
            Some(squared_l2_f64(query, actual_vector) - squared_l2_f64(query, expected_vector))
        }
        _ => None,
    };
    Comparison {
        missing_ids,
        extra_ids,
        rank_swaps,
        distance_gap,
    }
}

fn vector_for_id<'a>(collection: &'a CollectionView<'_>, id: u64) -> Option<&'a [f32]> {
    let index = collection
        .ids
        .iter()
        .position(|candidate| *candidate == id)?;
    Some(collection.vector(index))
}
