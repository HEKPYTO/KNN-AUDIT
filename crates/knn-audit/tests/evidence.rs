use serde_json::Value;

#[test]
fn first_pgvector_failure_matches_pinned_signature() {
    let scorecard: Value = serde_json::from_str(include_str!(
        "../../../results/scorecards/pgvector-0.8.7.json"
    ))
    .expect("scorecard");
    let signature: Value = serde_json::from_str(include_str!(
        "../../../fixtures/real-failure/pgvector-signature.json"
    ))
    .expect("signature");
    let failure = &scorecard["failures"][0];

    assert_eq!(signature["case_id"], failure["case_id"]);
    assert_eq!(signature["category"], failure["category"]);
    assert_eq!(signature["severity"], failure["severity"]);
    assert_eq!(signature["expected_ids"], failure["expected_ids"]);
    assert_eq!(signature["actual_ids"], failure["actual_ids"]);
    assert_eq!(signature["missing_ids"], failure["missing_ids"]);
    assert_eq!(signature["extra_ids"], failure["extra_ids"]);
    assert_eq!(signature["rank_swaps"], failure["rank_swaps"]);
    assert_eq!(signature["distance_gap"], failure["distance_gap"]);

    assert!(signature.get("query").is_none());
    assert!(signature.get("root").is_none());
    assert!(failure.get("query").is_none());
    assert!(failure.get("root").is_none());
}
