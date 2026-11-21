use knn_audit::adapters::CollectionView;
use knn_audit::compare::{Severity, compare};

fn view() -> CollectionView<'static> {
    CollectionView {
        dim: 1,
        ids: &[0, 1, 2, 3, 4, 5],
        vectors: &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
    }
}

#[test]
fn identical_lists_have_no_findings() {
    let result = compare(&[0.0], &view(), &[1, 2, 3], &[1, 2, 3]);
    assert!(result.missing_ids.is_empty());
    assert!(result.extra_ids.is_empty());
    assert!(result.rank_swaps.is_empty());
    assert_eq!(result.severity(false), Severity::Clean);
    assert_eq!(result.severity(true), Severity::Error);
    assert_eq!(result.distance_gap, None);
}

#[test]
fn rank_swaps_alone_are_order_only() {
    let result = compare(&[0.0], &view(), &[1, 2, 3], &[2, 1, 3]);
    assert!(result.missing_ids.is_empty());
    assert!(result.extra_ids.is_empty());
    assert_eq!(result.rank_swaps.len(), 2);
    assert_eq!(result.rank_swaps[0].id, 1);
    assert_eq!(result.rank_swaps[0].expected_rank, 1);
    assert_eq!(result.rank_swaps[0].actual_rank, 2);
    assert_eq!(result.severity(false), Severity::OrderOnly);
    assert_eq!(result.severity(true), Severity::Error);
}

#[test]
fn set_mismatches_are_blocking() {
    let result = compare(&[0.0], &view(), &[1, 2, 3], &[1, 4, 5]);
    assert_eq!(result.missing_ids, vec![2, 3]);
    assert_eq!(result.extra_ids, vec![4, 5]);
    assert!(result.rank_swaps.is_empty());
    assert_eq!(result.severity(false), Severity::Mismatch);
}

#[test]
fn duplicate_ids_count_as_missing_and_extra() {
    let result = compare(&[0.0], &view(), &[1, 2, 3], &[1, 1, 3]);
    assert_eq!(result.missing_ids, vec![2]);
    assert_eq!(result.extra_ids, vec![1]);
    assert_eq!(result.severity(false), Severity::Mismatch);
}

#[test]
fn empty_actual_ids_are_all_missing() {
    let result = compare(&[0.0], &view(), &[1, 2], &[]);
    assert_eq!(result.missing_ids, vec![1, 2]);
    assert!(result.extra_ids.is_empty());
    assert_eq!(result.distance_gap, None);
}

#[test]
fn distance_gap_uses_worst_extra_and_best_missing_boundary() {
    let result = compare(&[0.0], &view(), &[1, 2, 3], &[4, 1, 2]);
    assert_eq!(result.missing_ids, vec![3]);
    assert_eq!(result.extra_ids, vec![4]);
    assert_eq!(result.distance_gap, Some(16.0 - 9.0));
    assert_eq!(result.severity(false), Severity::Mismatch);
}

#[test]
fn unknown_ids_have_no_distance_gap() {
    let result = compare(&[0.0], &view(), &[99], &[3]);
    assert_eq!(result.missing_ids, vec![99]);
    assert_eq!(result.distance_gap, None);
}

#[test]
fn distance_gap_is_a_f64_diagnostic() {
    let result = compare(&[0.0], &view(), &[0], &[1]);
    assert_eq!(result.missing_ids, vec![0]);
    assert_eq!(result.extra_ids, vec![1]);
    assert_eq!(result.distance_gap, Some(1.0));
}
