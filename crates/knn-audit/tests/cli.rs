use assert_cmd::Command;
use knn_audit::adapters::{AdapterError, Caps, CollectionView, Engine, Hit, Metric};
use knn_audit::dataset::Dataset;
use knn_audit::run::{RunOptions, run};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

const MINI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mini");
const TIES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/ties");
const ORDER_ONLY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/order-only");

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_knn-audit"))
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

#[test]
fn dataset_verify_sample_exits_clean() {
    let output = command()
        .args(["dataset", "verify", "--dataset", MINI, "--sample", "2"])
        .output()
        .expect("verify command");
    assert!(output.status.success());
    assert!(stdout(&output).contains("0/2 cases mismatch"));
}

#[test]
fn dataset_verify_all_exits_clean() {
    let output = command()
        .args(["dataset", "verify", "--dataset", MINI, "--all"])
        .output()
        .expect("verify all");
    assert!(output.status.success());
    assert!(stdout(&output).contains("0/8 cases mismatch"));
}

#[test]
fn explicit_checksum_verification_controls_exit_code() {
    let hash = Dataset::load(MINI, None).expect("mini fixture").base_sha256;
    let matching = command()
        .args([
            "dataset",
            "verify",
            "--dataset",
            MINI,
            "--all",
            "--expect-sha256",
            &hash,
        ])
        .output()
        .expect("verify with hash");
    assert!(matching.status.success(), "{}", stdout(&matching));
    let mismatching = command()
        .args([
            "dataset",
            "verify",
            "--dataset",
            MINI,
            "--all",
            "--expect-sha256",
            "deadbeef",
        ])
        .output()
        .expect("verify with bad hash");
    assert_eq!(mismatching.status.code(), Some(2));
    let stderr = String::from_utf8(mismatching.stderr).expect("utf8 stderr");
    assert!(stderr.contains("sha256 mismatch"));
}

#[test]
fn run_rejects_wrong_explicit_checksum() {
    let temp = tempfile::tempdir().expect("temp output");
    let output = command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "reference",
            "--expect-sha256",
            "deadbeef",
            "--out",
            temp.path().join("report.json").to_str().expect("out"),
        ])
        .output()
        .expect("run with bad hash");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("sha256 mismatch"));
}
#[test]
fn conflicting_sample_flags_are_usage_errors() {
    let output = command()
        .args([
            "dataset",
            "verify",
            "--dataset",
            MINI,
            "--sample",
            "2",
            "--all",
        ])
        .output()
        .expect("conflicting flags");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn reference_run_writes_report_and_html() {
    let temp = tempfile::tempdir().expect("temp output");
    let json = temp.path().join("reference.json");
    let html = temp.path().join("reference.html");
    let output = command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "reference",
            "--limit",
            "20",
            "--out",
            json.to_str().expect("json path"),
            "--html",
            html.to_str().expect("html path"),
        ])
        .output()
        .expect("run command");
    assert!(output.status.success(), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(text.contains("0/8 cases blocked"), "{text}");
    let report: Value =
        serde_json::from_slice(&fs::read(&json).expect("report json")).expect("parse report");
    assert_eq!(report["schema"], "knn-audit/v1");
    assert_eq!(report["engine"]["name"], "reference");
    assert_eq!(report["engine"]["exact_mode"], true);
    assert_eq!(report["summary"]["cases"], 8);
    assert_eq!(report["summary"]["failed"], 0);
    assert_eq!(report["failures"].as_array().expect("failures").len(), 0);
    assert_eq!(
        report["categories"].as_array().expect("categories").len(),
        1
    );
    let markup = fs::read_to_string(&html).expect("html");
    assert!(markup.contains("knn-audit scorecard"));
    assert!(!markup.contains("<script"));
}

#[test]
fn reference_run_handles_ties_and_extreme_values() {
    let temp = tempfile::tempdir().expect("temp output");
    let json = temp.path().join("ties.json");
    let output = command()
        .args([
            "run",
            "--dataset",
            TIES,
            "--engine",
            "reference",
            "--out",
            json.to_str().expect("json path"),
        ])
        .output()
        .expect("ties run");
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(stdout(&output).contains("0/4 cases blocked"));
}

#[test]
fn fail_on_mismatch_controls_exit_code() {
    let temp = tempfile::tempdir().expect("temp dataset");
    let data = temp.path().join("data");
    let group = data.join("g");
    fs::create_dir_all(&group).expect("group");
    fs::write(data.join("base.csv"), "0,0\n1,0\n2,0\n").expect("base");
    fs::write(
        group.join("bad.json"),
        "{\"case_id\":\"bad\",\"category\":\"g\",\"query_row\":0,\"query\":[0,0],\"gt_ids\":[1,2]}",
    )
    .expect("case");
    let out = temp.path().join("bad.json");
    let base_args = [
        "run",
        "--dataset",
        temp.path().to_str().expect("dataset path"),
        "--engine",
        "reference",
        "--k",
        "2",
        "--out",
        out.to_str().expect("out path"),
    ];
    let clean = command()
        .args(base_args)
        .output()
        .expect("run without flag");
    assert_eq!(clean.status.code(), Some(0));
    let failing = command()
        .args(base_args)
        .arg("--fail-on-mismatch")
        .output()
        .expect("run with flag");
    assert_eq!(failing.status.code(), Some(1));
    let report: Value =
        serde_json::from_slice(&fs::read(&out).expect("report")).expect("parse report");
    assert_eq!(report["summary"]["failed"], 1);
    assert_eq!(report["failures"][0]["case_id"], "bad");
    assert_eq!(report["failures"][0]["missing_ids"], serde_json::json!([2]));
}

#[test]
fn unavailable_engine_lists_available_names() {
    let output = command()
        .args(["run", "--dataset", MINI, "--engine", "bogus"])
        .output()
        .expect("unavailable engine");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("reference"));
    assert!(stderr.contains("qdrant"));
    assert!(stderr.contains("pgvector"));
    assert!(stderr.contains("faiss"));
}

#[test]
fn zero_repeat_is_a_usage_error() {
    let output = command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "reference",
            "--repeat",
            "0",
        ])
        .output()
        .expect("repeat validation");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("--repeat must be positive"));
}

#[test]
fn repeat_summary_line_reports_any_and_every_run() {
    let temp = tempfile::tempdir().expect("temp output");
    let out = temp.path().join("repeat.json");
    let output = command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "reference",
            "--repeat",
            "2",
            "--out",
            out.to_str().expect("out path"),
        ])
        .output()
        .expect("repeat audit");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("reference 0.1.0 (repeat 2): 0/8 cases blocked"));
    assert!(stdout.contains(
        "0 in every run, 0 order-only, 0 unstable; 0 errors; 0 missing, 0 extra, 0 rank swaps"
    ));
}

#[test]
fn local_jobs_produce_the_same_report() {
    let temp = tempfile::tempdir().expect("temp output");
    let mut reports = Vec::new();
    for (jobs, name) in [("1", "one.json"), ("2", "two.json")] {
        let out = temp.path().join(name);
        command()
            .args([
                "run",
                "--dataset",
                MINI,
                "--engine",
                "reference",
                "--repeat",
                "2",
                "--jobs",
                jobs,
                "--out",
                out.to_str().expect("out"),
            ])
            .assert()
            .success();
        let mut report: Value =
            serde_json::from_slice(&fs::read(&out).expect("report")).expect("parse report");
        report.as_object_mut().expect("object").remove("created_at");
        reports.push(report);
    }
    assert_eq!(reports[0], reports[1]);
}

#[test]
fn qdrant_dispatch_reports_adapter_error() {
    let output = command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "qdrant",
            "--engine-url",
            "http://127.0.0.1:1",
            "--collection",
            "audit_dead",
        ])
        .output()
        .expect("qdrant dispatch");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("qdrant prepare failed"));
}

#[test]
fn pgvector_dispatch_reports_adapter_error() {
    let output = command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "pgvector",
            "--database-url",
            "postgres://postgres:postgres@127.0.0.1:1/postgres?connect_timeout=1",
            "--collection",
            "audit_dead",
        ])
        .output()
        .expect("pgvector dispatch");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("pgvector connection failed"));
}

#[test]
fn report_command_reads_json_and_renders_html() {
    let temp = tempfile::tempdir().expect("temp output");
    let json = temp.path().join("report.json");
    command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "reference",
            "--out",
            json.to_str().expect("json path"),
        ])
        .assert()
        .success();
    let html = temp.path().join("report.html");
    let output = command()
        .args([
            "report",
            "--input",
            json.to_str().expect("input"),
            "--html",
            html.to_str().expect("html"),
        ])
        .output()
        .expect("report command");
    assert!(output.status.success());
    assert!(stdout(&output).contains("knn-audit/v1"));
    assert!(fs::read_to_string(&html).expect("html").contains("<table>"));
}

#[test]
fn report_command_rejects_unknown_schema() {
    let temp = tempfile::tempdir().expect("temp report");
    let path = temp.path().join("report.json");
    fs::write(&path, "{\"schema\":\"other\"}").expect("report");
    let output = command()
        .args(["report", "--input", path.to_str().expect("path")])
        .output()
        .expect("report command");
    assert_eq!(output.status.code(), Some(2));
}

const PINNED_BASE_SHA256: &str = "c0158e652bf8bcd4ad8a42aa8eb642325ca76897109431241ada0ac7490d4beb";
const SECRET_ROOT: &str = "/private/tmp/errpedia-secret-dataset";

fn publish_report() -> Value {
    serde_json::json!({
        "schema": "knn-audit/v1",
        "tool": {"name": "knn-audit", "version": "0.1.0"},
        "created_at": "2026-11-21T00:00:00Z",
        "dataset": {
            "root": SECRET_ROOT,
            "base_sha256": "abc123",
            "base_rows": 32,
            "cases": 3,
            "dim": 4
        },
        "engine": {
            "name": "reference",
            "version": "0.1.0",
            "exact_mode": true,
            "metric": "squared_l2",
            "config": {
                "kernel": "dyadic-rational",
                "screening": "certified-f64",
                "tie_break": "smaller-id"
            }
        },
        "k": 2,
        "repeat": 2,
        "runs": [{"failed": 3, "order_only": 0, "errors": 0, "missing": 3, "extra": 2, "rank_swaps": 1}],
        "summary": {
            "cases": 3, "passed": 0, "failed": 3, "order_only": 0, "errors": 0,
            "missing": 3, "extra": 2, "rank_swaps": 1, "unstable": 1, "failed_all": 2
        },
        "categories": [
            {"category": "zzz", "cases": 1, "failed": 1, "order_only": 0},
            {"category": "aaa", "cases": 2, "failed": 2, "order_only": 0}
        ],
        "failures": [
            {
                "case_id": "case-b", "category": "aaa",
                "query": [0.1, 0.2, 0.3, 0.4], "severity": "mismatch",
                "expected_ids": [7, 8], "actual_ids": [7, 9],
                "missing_ids": [8], "extra_ids": [9],
                "rank_swaps": [{"id": 7, "expected_rank": 1, "actual_rank": 2}],
                "distance_gap": 0.125, "error": null,
                "failed_repetitions": [1, 2], "unstable": true,
                "unstable_actual_ids": [[7, 9], [9, 7]]
            },
            {
                "case_id": "case-a", "category": "zzz",
                "query": [1.0, 0.0, 0.0, 0.0], "severity": "error",
                "expected_ids": [1, 2], "actual_ids": [1, 3],
                "missing_ids": [2], "extra_ids": [3],
                "rank_swaps": [], "distance_gap": null,
                "error": "timeout", "failed_repetitions": [2], "unstable": false,
                "unstable_actual_ids": []
            },
            {
                "case_id": "case-a", "category": "aaa",
                "query": [0.0, 1.0, 0.0, 0.0], "severity": "mismatch",
                "expected_ids": [4, 5], "actual_ids": [4, 6],
                "missing_ids": [5], "extra_ids": [6],
                "rank_swaps": [], "distance_gap": null, "error": null,
                "failed_repetitions": [1], "unstable": false, "unstable_actual_ids": []
            }
        ]
    })
}

fn collect(value: &Value, keys: &mut Vec<String>, strings: &mut Vec<String>) {
    match value {
        Value::Object(entries) => {
            for (key, child) in entries {
                keys.push(key.clone());
                collect(child, keys, strings);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect(child, keys, strings);
            }
        }
        Value::String(text) => strings.push(text.clone()),
        _ => {}
    }
}

#[test]
fn publish_writes_redacted_deterministic_scorecard() {
    let temp = tempfile::tempdir().expect("temp report");
    let input = temp.path().join("report.json");
    fs::write(&input, publish_report().to_string()).expect("report");
    let first = temp.path().join("first.json");
    let second = temp.path().join("second.json");
    for out in [&first, &second] {
        command()
            .args([
                "publish",
                "--input",
                input.to_str().expect("input"),
                "--out",
                out.to_str().expect("out"),
            ])
            .assert()
            .success();
    }
    let first_text = fs::read_to_string(&first).expect("first scorecard");
    assert_eq!(
        first_text,
        fs::read_to_string(&second).expect("second scorecard")
    );
    let scorecard: Value = serde_json::from_str(&first_text).expect("scorecard json");
    assert_eq!(scorecard["schema"], "knn-audit-scorecard/v1");
    assert_eq!(scorecard["tool"]["name"], "knn-audit");
    assert_eq!(scorecard["tool"]["version"], "0.1.0");
    assert_eq!(scorecard["dataset"]["base_sha256"], "abc123");
    assert_eq!(scorecard["dataset"]["base_rows"], 32);
    assert_eq!(scorecard["dataset"]["cases"], 3);
    assert_eq!(scorecard["dataset"]["dim"], 4);
    assert_eq!(scorecard["engine"]["name"], "reference");
    assert_eq!(scorecard["engine"]["metric"], "squared_l2");
    assert_eq!(scorecard["summary"]["order_only"], 0);
    assert_eq!(
        scorecard["engine"]["config"],
        serde_json::json!({
            "kernel": "dyadic-rational",
            "screening": "certified-f64",
            "tie_break": "smaller-id"
        })
    );
    assert_eq!(scorecard["k"], 2);
    assert_eq!(scorecard["repeat"], 2);
    assert_eq!(scorecard["runs"].as_array().expect("runs").len(), 1);
    assert_eq!(scorecard["summary"]["failed"], 3);
    assert_eq!(scorecard["categories"][0]["category"], "aaa");
    assert_eq!(scorecard["categories"][1]["category"], "zzz");
    assert_eq!(scorecard["failures"][0]["case_id"], "case-a");
    assert_eq!(scorecard["failures"][0]["category"], "aaa");
    assert_eq!(
        scorecard["failures"][0]["expected_ids"],
        serde_json::json!([4, 5])
    );
    assert_eq!(
        scorecard["failures"][2]["expected_ids"],
        serde_json::json!([7, 8])
    );
    let mut keys = Vec::new();
    let mut strings = Vec::new();
    collect(&scorecard, &mut keys, &mut strings);
    assert!(!keys.iter().any(|key| {
        key == "query" || key == "root" || key == "config_redacted" || key == "created_at"
    }));
    assert!(
        scorecard["failures"]
            .as_array()
            .expect("failures")
            .iter()
            .all(|failure| failure["unstable"].is_boolean())
    );
    assert!(
        !strings
            .iter()
            .any(|text| text.contains(SECRET_ROOT) || text.contains("secret-token"))
    );
    assert!(!first_text.contains(SECRET_ROOT));
    let help = command().args(["--help"]).output().expect("help");
    assert!(stdout(&help).contains("publish"));
}

#[test]
fn publish_rejects_wrong_schema() {
    let temp = tempfile::tempdir().expect("temp report");
    let input = temp.path().join("report.json");
    let mut report = publish_report();
    report["schema"] = serde_json::json!("other");
    fs::write(&input, report.to_string()).expect("report");
    let output = command()
        .args([
            "publish",
            "--input",
            input.to_str().expect("input"),
            "--out",
            temp.path().join("scorecard.json").to_str().expect("out"),
        ])
        .output()
        .expect("publish");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn publish_missing_input_exits_two() {
    let temp = tempfile::tempdir().expect("temp report");
    let input = temp.path().join("absent.json");
    let output = command()
        .args([
            "publish",
            "--input",
            input.to_str().expect("input"),
            "--out",
            temp.path().join("scorecard.json").to_str().expect("out"),
        ])
        .output()
        .expect("publish");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn rank_swap_only_does_not_fail_with_flag() {
    let temp = tempfile::tempdir().expect("temp output");
    let out = temp.path().join("order.json");
    let output = command()
        .args([
            "run",
            "--dataset",
            ORDER_ONLY,
            "--engine",
            "reference",
            "--k",
            "3",
            "--out",
            out.to_str().expect("out path"),
            "--fail-on-mismatch",
        ])
        .output()
        .expect("order-only run");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let text = stdout(&output);
    assert!(
        text.contains("0/1 cases blocked (0 in every run, 1 order-only, 0 unstable; 0 errors; 0 missing, 0 extra, 2 rank swaps)"),
        "{text}"
    );
    let report: Value =
        serde_json::from_slice(&fs::read(&out).expect("report")).expect("parse report");
    assert_eq!(report["summary"]["failed"], 0);
    assert_eq!(report["summary"]["order_only"], 1);
    assert_eq!(report["summary"]["missing"], 0);
    assert_eq!(report["summary"]["extra"], 0);
    assert_eq!(report["summary"]["rank_swaps"], 2);
    assert_eq!(report["failures"][0]["severity"], "order_only");
    assert_eq!(report["failures"][0]["rank_swaps"][0]["expected_rank"], 2);
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
fn real_dataset_verify_all_reports_clean() {
    let Some(path) = real_dataset() else {
        return;
    };
    let output = command()
        .args([
            "dataset",
            "verify",
            "--dataset",
            path.to_str().expect("dataset path"),
            "--all",
            "--expect-sha256",
            PINNED_BASE_SHA256,
        ])
        .output()
        .expect("verify all");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("0/1616 cases mismatch"));
}

#[test]
fn real_reference_run_reports_clean() {
    let Some(path) = real_dataset() else {
        return;
    };
    let temp = tempfile::tempdir().expect("temp output");
    let out = temp.path().join("reference.json");
    let output = command()
        .args([
            "run",
            "--dataset",
            path.to_str().expect("dataset path"),
            "--engine",
            "reference",
            "--expect-sha256",
            PINNED_BASE_SHA256,
            "--out",
            out.to_str().expect("out path"),
        ])
        .output()
        .expect("reference run");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("(repeat 1): 0/1616 cases blocked (0 in every run, 0 order-only, 0 unstable; 0 errors; 0 missing, 0 extra, 0 rank swaps)"));
}

struct ProvenanceEngine;

impl Engine for ProvenanceEngine {
    fn name(&self) -> &'static str {
        "provenance"
    }

    fn version(&self) -> String {
        "test".to_string()
    }

    fn prepare(&mut self, _collection: &CollectionView<'_>) -> Result<(), AdapterError> {
        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<Hit>, AdapterError> {
        Ok(query
            .iter()
            .take(k)
            .enumerate()
            .map(|(id, _)| Hit {
                id: id.to_string(),
                distance: None,
            })
            .collect())
    }

    fn capabilities(&self) -> Caps {
        Caps {
            exact_mode: true,
            distance_metric: Metric::SquaredL2,
        }
    }

    fn config(&self) -> std::collections::BTreeMap<String, String> {
        std::collections::BTreeMap::from([
            ("kernel".to_string(), "dyadic-rational".to_string()),
            ("screening".to_string(), "certified-f64".to_string()),
        ])
    }
}

#[test]
fn engine_config_flows_from_run_to_published_scorecard() {
    let report = run(RunOptions {
        dataset: Dataset::load(MINI, None).expect("mini fixture"),
        engine: Box::new(ProvenanceEngine),
        k: 2,
        limit: Some(2),
        repeat: 1,
        jobs: None,
    })
    .expect("run")
    .report;
    let expected = serde_json::json!({
        "kernel": "dyadic-rational",
        "screening": "certified-f64"
    });
    assert_eq!(
        serde_json::to_value(&report.engine.config).expect("config json"),
        expected
    );
    let temp = tempfile::tempdir().expect("temp output");
    let input = temp.path().join("report.json");
    let out = temp.path().join("scorecard.json");
    fs::write(&input, serde_json::to_string(&report).expect("report json")).expect("report");
    command()
        .args([
            "publish",
            "--input",
            input.to_str().expect("input"),
            "--out",
            out.to_str().expect("out"),
        ])
        .assert()
        .success();
    let text = fs::read_to_string(&out).expect("scorecard");
    let scorecard: Value = serde_json::from_str(&text).expect("scorecard json");
    assert_eq!(scorecard["engine"]["config"], expected);
    assert_eq!(scorecard["tool"]["name"], "knn-audit");
    assert_eq!(scorecard["tool"]["version"], "0.1.0");
    assert_eq!(scorecard["summary"]["order_only"], 0);
    let mut keys = Vec::new();
    let mut strings = Vec::new();
    collect(&scorecard, &mut keys, &mut strings);
    assert!(!keys.iter().any(|key| {
        key == "query" || key == "root" || key == "config_redacted" || key == "created_at"
    }));
    let config = scorecard["engine"]["config"].as_object().expect("config");
    assert!(config.iter().all(|(key, value)| {
        ["http", "postgres", "password", "/"]
            .iter()
            .all(|marker| !key.contains(marker) && !value.to_string().contains(marker))
    }));
    assert!(!text.contains("secret"));
}
