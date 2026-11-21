use assert_cmd::Command;
use serde_json::Value;
use std::fs;

fn write_dataset(root: &std::path::Path) {
    let data = root.join("data");
    fs::create_dir_all(data.join("a")).expect("category a");
    fs::create_dir_all(data.join("z")).expect("category z");
    fs::write(data.join("base.csv"), "0,0\n1,0\n2,0\n").expect("base");
    fs::write(
        data.join("z").join("z_case.json"),
        "{\"case_id\":\"<script>alert(\\\"x\\\")</script>\",\"category\":\"z\",\"query_row\":0,\"query\":[0,0],\"gt_ids\":[1,2]}",
    )
    .expect("z case");
    fs::write(
        data.join("a").join("a_case.json"),
        "{\"case_id\":\"a_case\",\"category\":\"a\",\"query_row\":1,\"query\":[1,0],\"gt_ids\":[0,2]}",
    )
    .expect("a case");
}

#[test]
fn json_schema_is_ordered_and_failures_are_sorted() {
    let temp = tempfile::tempdir().expect("temp dataset");
    write_dataset(temp.path());
    let json = temp.path().join("report.json");
    Command::new(env!("CARGO_BIN_EXE_knn-audit"))
        .args([
            "run",
            "--dataset",
            temp.path().to_str().expect("dataset"),
            "--engine",
            "reference",
            "--k",
            "2",
            "--out",
            json.to_str().expect("json"),
        ])
        .assert()
        .success();
    let text = fs::read_to_string(&json).expect("report");
    let keys = [
        "schema",
        "tool",
        "created_at",
        "dataset",
        "engine",
        "k",
        "summary",
        "categories",
        "failures",
    ];
    let mut previous = 0;
    for key in keys {
        let position = text.find(&format!("\"{key}\"")).expect("key present");
        assert!(position > previous, "{key} is out of schema order");
        previous = position;
    }
    let report: Value = serde_json::from_str(&text).expect("parse report");
    assert_eq!(report["schema"], "knn-audit/v1");
    assert_eq!(report["dataset"]["base_rows"], 3);
    assert_eq!(report["dataset"]["dim"], 2);
    assert_eq!(report["dataset"]["cases"], 2);
    assert_eq!(report["k"], 2);
    assert_eq!(report["summary"]["cases"], 2);
    assert_eq!(report["summary"]["failed"], 2);
    let categories = report["categories"].as_array().expect("categories");
    assert_eq!(categories[0]["category"], "a");
    assert_eq!(categories[1]["category"], "z");
    let failures = report["failures"].as_array().expect("failures");
    assert_eq!(failures[0]["case_id"], "<script>alert(\"x\")</script>");
    assert_eq!(failures[1]["case_id"], "a_case");
    assert_eq!(failures[1]["missing_ids"], serde_json::json!([2]));
}

#[test]
fn html_escapes_untrusted_case_ids() {
    let temp = tempfile::tempdir().expect("temp dataset");
    write_dataset(temp.path());
    let json = temp.path().join("report.json");
    let html = temp.path().join("report.html");
    Command::new(env!("CARGO_BIN_EXE_knn-audit"))
        .args([
            "run",
            "--dataset",
            temp.path().to_str().expect("dataset"),
            "--engine",
            "reference",
            "--k",
            "2",
            "--out",
            json.to_str().expect("json"),
            "--html",
            html.to_str().expect("html"),
        ])
        .assert()
        .success();
    let markup = fs::read_to_string(&html).expect("html");
    assert!(markup.contains("&lt;script&gt;"));
    assert!(!markup.contains("<script"));
    assert!(!markup.contains("http://") && !markup.contains("https://"));
}
