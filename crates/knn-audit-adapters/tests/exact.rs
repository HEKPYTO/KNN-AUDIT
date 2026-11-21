use knn_audit_adapters::CollectionView;
use knn_audit_adapters::exact::{squared_l2_exact, top_k_exact};
use std::cmp::Ordering;

fn subnormal() -> f32 {
    f32::from_bits(1)
}

#[test]
fn zero_and_negative_zero_are_exactly_zero() {
    let distance = squared_l2_exact(&[0.0, -0.0], &[-0.0, 0.0]);
    assert_eq!(distance.cmp_f64(0.0), Ordering::Equal);
}

#[test]
fn subnormal_difference_is_not_absorbed() {
    let tiny = subnormal();
    let distance = squared_l2_exact(&[tiny, 0.0], &[0.0, 0.0]);
    assert_eq!(distance.cmp_f64(0.0), Ordering::Greater);
    let wider = squared_l2_exact(&[2.0 * tiny, 0.0], &[0.0, 0.0]);
    assert!(wider > distance);
}

#[test]
fn overflow_sized_coordinates_keep_exact_order() {
    let near = squared_l2_exact(&[f32::MAX], &[0.0]);
    let far = squared_l2_exact(&[f32::MAX], &[-f32::MAX]);
    assert!(far > near);
    assert_eq!(near.cmp_f64(0.0), Ordering::Greater);
}

#[test]
fn absorption_does_not_hide_a_small_coordinate() {
    let tiny = subnormal();
    let absorbed = squared_l2_exact(&[1.0e8_f32, tiny], &[1.0e8_f32, 0.0]);
    assert!(absorbed > knn_audit_adapters::exact::ExactDistance::zero());
}

#[test]
fn exact_ties_and_near_ties_are_distinguished() {
    let left = squared_l2_exact(&[0.5], &[0.0]);
    let right = squared_l2_exact(&[0.5], &[1.0]);
    let near = squared_l2_exact(&[0.5], &[1.0000001]);
    assert_eq!(left.cmp(&right), Ordering::Equal);
    assert_eq!(left.cmp(&near), Ordering::Less);
}

#[test]
fn top_k_uses_exact_order_then_smaller_id() {
    let ids = [10, 11, 12, 13, 14];
    let vectors = [0.0_f32, 1.0, -1.0, 2.0, -2.0];
    let collection = CollectionView {
        dim: 1,
        ids: &ids,
        vectors: &vectors,
    };
    assert_eq!(
        top_k_exact(&[0.1], &collection, 3).expect("top k"),
        vec![10, 11, 12]
    );
    assert_eq!(
        top_k_exact(&[0.0], &collection, 10).expect("top k"),
        vec![10, 11, 12, 13, 14]
    );
}

#[test]
fn top_k_splitting_a_tie_group_returns_smaller_ids() {
    let ids = [9, 3, 7, 1, 5];
    let vectors = [0.0_f32; 5];
    let collection = CollectionView {
        dim: 1,
        ids: &ids,
        vectors: &vectors,
    };
    assert_eq!(
        top_k_exact(&[1.0], &collection, 2).expect("top k"),
        vec![1, 3]
    );
    assert_eq!(
        top_k_exact(&[1.0], &collection, 3).expect("top k"),
        vec![1, 3, 5]
    );
}

#[test]
fn top_k_with_zero_dimension_returns_empty() {
    let collection = CollectionView {
        dim: 0,
        ids: &[1],
        vectors: &[],
    };
    assert!(top_k_exact(&[], &collection, 5).expect("empty").is_empty());
}

#[test]
fn top_k_reports_query_and_collection_shape_errors() {
    let query_error = top_k_exact(
        &[0.0],
        &CollectionView {
            dim: 2,
            ids: &[1],
            vectors: &[0.0, 0.0],
        },
        1,
    )
    .expect_err("query dimension mismatch");
    assert!(query_error.message.contains("query dimension"));
    let shape_error = top_k_exact(
        &[0.0, 0.0],
        &CollectionView {
            dim: 2,
            ids: &[1, 2],
            vectors: &[0.0, 0.0, 1.0],
        },
        1,
    )
    .expect_err("shape mismatch");
    assert!(shape_error.message.contains("collection size mismatch"));
}

#[test]
fn top_k_matches_brute_force_exact_distances() {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let ids: Vec<u64> = (0..32).collect();
    for case in 0..50 {
        let mut query = [0.0_f32; 8];
        let mut vectors = vec![0.0_f32; ids.len() * 8];
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as i64) as f32 / i32::MAX as f32
        };
        for value in &mut query {
            *value = next();
        }
        for value in &mut vectors {
            *value = next();
        }
        let collection = CollectionView {
            dim: 8,
            ids: &ids,
            vectors: &vectors,
        };
        let k = 1 + case % 10;
        let mut ranked: Vec<_> = (0..ids.len())
            .map(|index| {
                (
                    squared_l2_exact(&query, collection.vector(index)),
                    ids[index],
                )
            })
            .collect();
        ranked.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        let expected: Vec<u64> = ranked.into_iter().take(k).map(|(_, id)| id).collect();
        assert_eq!(
            top_k_exact(&query, &collection, k).expect("top k"),
            expected,
            "case {case}"
        );
    }
}
