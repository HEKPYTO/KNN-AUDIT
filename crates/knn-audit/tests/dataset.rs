use knn_audit::dataset::{Dataset, EXPECTED_BASE_SHA256};
use std::fs;
use std::path::PathBuf;

const MINI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mini");

fn fixture(base: &str, case: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp dataset");
    let group = root.path().join("data").join("g");
    fs::create_dir_all(&group).expect("case group");
    fs::write(root.path().join("data").join("base.csv"), base).expect("base.csv");
    fs::write(group.join("case.json"), case).expect("case.json");
    root
}

fn valid_case(gt_ids: &str) -> String {
    format!(
        "{{\"case_id\":\"case\",\"category\":\"g\",\"query_row\":0,\"query\":[0,0],\"gt_ids\":{gt_ids}}}"
    )
}

#[test]
fn loads_mini_fixture_and_verifies_all_cases() {
    let dataset = Dataset::load(MINI).expect("mini fixture");
    assert_eq!(dataset.base_rows, 64);
    assert_eq!(dataset.dim, 8);
    assert_eq!(dataset.cases.len(), 8);
    assert_eq!(dataset.ids.first(), Some(&0));
    assert_eq!(dataset.ids.last(), Some(&63));
    let indices: Vec<usize> = (0..dataset.cases.len()).collect();
    let summary = dataset.verify(&indices, 10).expect("verify");
    assert_eq!(summary.mismatches, Vec::<String>::new());
}

#[test]
fn checksum_mismatch_is_reported() {
    let dataset = Dataset::load(MINI).expect("mini fixture");
    assert!(dataset.check_checksum(&dataset.base_sha256).is_ok());
    let error = dataset
        .check_checksum("deadbeef")
        .expect_err("bad checksum");
    assert!(error.message.contains("sha256 mismatch"));
}

#[test]
fn malformed_case_json_fails_loudly() {
    let root = fixture("0,0\n1,0\n", "{not json");
    let error = Dataset::load(root.path()).expect_err("malformed json");
    assert!(error.message.contains("malformed case"));
}

#[test]
fn ground_truth_id_out_of_range_fails_loudly() {
    let root = fixture("0,0\n1,0\n", &valid_case("[9]"));
    let error = Dataset::load(root.path()).expect_err("out of range id");
    assert!(error.message.contains("out of range"));
}

#[test]
fn ragged_base_rows_fail_loudly() {
    let root = fixture("0,0\n1,0,0\n", &valid_case("[0]"));
    let error = Dataset::load(root.path()).expect_err("ragged rows");
    assert!(error.message.contains("expected 2"));
}

#[test]
fn non_finite_coordinates_fail_loudly() {
    let root = fixture("NaN,0\n1,0\n", &valid_case("[0]"));
    let error = Dataset::load(root.path()).expect_err("non finite");
    assert!(error.message.contains("non-finite"));
}

#[test]
fn verify_rejects_out_of_range_case_index() {
    let dataset = Dataset::load(MINI).expect("mini fixture");
    let error = dataset
        .verify(&[dataset.cases.len()], 10)
        .expect_err("out of range index");
    assert!(error.message.contains("out of range"));
}

fn real_dataset() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("ERRPEDIA_DATASET") {
        candidates.push(PathBuf::from(path));
    }
    candidates.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../.data/errpedia"
    )));
    candidates
        .into_iter()
        .find(|path| path.join("data").join("base.csv").exists())
}

#[test]
fn real_dataset_reproduces_sampled_ground_truth() {
    let Some(path) = real_dataset() else {
        return;
    };
    let dataset = Dataset::load(&path).expect("real dataset");
    assert!(dataset.is_shipped());
    dataset
        .check_checksum(EXPECTED_BASE_SHA256)
        .expect("pinned checksum");
    let summary = dataset
        .verify(&[0, 100, 400, 800, 1200, 1600], 10)
        .expect("verify");
    assert_eq!(summary.mismatches, Vec::<String>::new());
}

#[test]
fn real_absorb_001_matches_exact_kernel() {
    let Some(path) = real_dataset() else {
        return;
    };
    let dataset = Dataset::load(&path).expect("real dataset");
    let case = dataset
        .cases
        .iter()
        .find(|case| case.case_id == "absorb_001")
        .expect("absorb_001 case");
    let mut expected = case.gt_ids.clone();
    let mut actual = knn_audit::exact::top_k_exact(&case.query, &dataset.collection(), 10);
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}
