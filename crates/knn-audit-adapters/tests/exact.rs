use knn_audit_adapters::CollectionView;
use knn_audit_adapters::exact::{distance_interval, squared_l2_exact, squared_l2_f64, top_k_exact};
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
    let computed = squared_l2_f64(&[tiny, 0.0], &[0.0, 0.0]);
    let (lower, upper) = distance_interval(computed, 2);
    assert!(distance.cmp_f64(lower) != Ordering::Less);
    assert!(distance.cmp_f64(upper) != Ordering::Greater);
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
    let computed = squared_l2_f64(&[1.0e8_f32, tiny], &[1.0e8_f32, 0.0]);
    let (lower, upper) = distance_interval(computed, 2);
    assert!(absorbed.cmp_f64(lower) != Ordering::Less);
    assert!(absorbed.cmp_f64(upper) != Ordering::Greater);
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
fn f64_interval_contains_exact_two_sided() {
    let mut state = 0x1234_5678_9abc_def0_u64;
    for case in 0..500 {
        let mut query = [0.0_f32; 8];
        let mut vector = [0.0_f32; 8];
        for index in 0..8 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let raw = ((state >> 33) as i64) as f32 / i32::MAX as f32;
            let scale = if case % 3 == 0 { 1.0e10_f32 } else { 1.0_f32 };
            query[index] = raw * scale;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let raw_other = ((state >> 33) as i64) as f32 / i32::MAX as f32;
            vector[index] = raw_other * scale;
        }
        let exact = squared_l2_exact(&query, &vector);
        let computed = squared_l2_f64(&query, &vector);
        let (lower, upper) = distance_interval(computed, query.len());
        assert!(
            exact.cmp_f64(lower) != Ordering::Less && exact.cmp_f64(upper) != Ordering::Greater,
            "case {case}: f64 interval [{lower}, {upper}] does not contain the exact value"
        );
    }
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
    assert_eq!(top_k_exact(&[0.1], &collection, 3), vec![10, 11, 12]);
    assert_eq!(
        top_k_exact(&[0.0], &collection, 10),
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
    assert_eq!(top_k_exact(&[1.0], &collection, 2), vec![1, 3]);
    assert_eq!(top_k_exact(&[1.0], &collection, 3), vec![1, 3, 5]);
}

#[test]
fn top_k_with_zero_dimension_returns_empty() {
    let collection = CollectionView {
        dim: 0,
        ids: &[1],
        vectors: &[],
    };
    assert!(top_k_exact(&[], &collection, 5).is_empty());
}
