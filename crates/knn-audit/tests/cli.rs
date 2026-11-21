use assert_cmd::Command;
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
    assert!(text.contains("0/8 cases mismatch"), "{text}");
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
    assert!(stdout(&output).contains("0/4 cases mismatch"));
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
    assert!(stdout.contains("reference 0.1.0 (repeat 2): 0/8 cases mismatch any"));
    assert!(stdout.contains("0 in every run, 0 unstable; 0 missing, 0 extra, 0 rank swaps"));
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
    assert!(stderr.contains("pgvector prepare failed"));
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
        text.contains(
            "1/1 cases mismatch any (1 in every run, 0 unstable; 0 missing, 0 extra, 2 rank swaps)"
        ),
        "{text}"
    );
    let report: Value =
        serde_json::from_slice(&fs::read(&out).expect("report")).expect("parse report");
    assert_eq!(report["summary"]["failed"], 1);
    assert_eq!(report["summary"]["missing"], 0);
    assert_eq!(report["summary"]["extra"], 0);
    assert_eq!(report["summary"]["rank_swaps"], 2);
    assert_eq!(report["failures"][0]["rank_swaps"][0]["expected_rank"], 2);
}

fn real_dataset() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("ERRPEDIA_DATASET") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([
        PathBuf::from("../../../.data/errpedia"),
        PathBuf::from(".data/errpedia"),
    ]);
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
            "--out",
            out.to_str().expect("out path"),
        ])
        .output()
        .expect("reference run");
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("0/1616 cases mismatch (0 missing, 0 extra, 0 rank swaps)"));
}

const FAKE_FAISS_WORKER: &str = r#"#!/usr/bin/env python3
import json
import sys

state = {"dim": 0, "ids": [], "vectors": []}

for line in sys.stdin:
    request = json.loads(line)
    op = request["op"]
    if op == "version":
        response = {"version": "fake-faiss-1"}
    elif op == "create":
        state["dim"] = request["dim"]
        state["ids"] = []
        state["vectors"] = []
        response = {"ok": True}
    elif op == "add":
        state["ids"].extend(request["ids"])
        state["vectors"].extend(request["vectors"])
        response = {"ok": True}
    elif op == "count":
        response = {"count": len(state["ids"])}
    elif op == "search":
        dim = state["dim"]
        query = request["query"]
        scored = []
        for index, dataset_id in enumerate(state["ids"]):
            vector = state["vectors"][index * dim:(index + 1) * dim]
            distance = sum((a - b) ** 2 for a, b in zip(query, vector))
            scored.append((distance, dataset_id))
        scored.sort()
        response = [{"id": item[1], "distance": item[0]} for item in scored[:request["k"]]]
    elif op == "reset":
        state["ids"] = []
        state["vectors"] = []
        response = {"ok": True}
    elif op == "close":
        response = {"ok": True}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()
    if op == "close":
        break
"#;

#[test]
fn faiss_dispatch_uses_python_protocol_worker() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temp output");
    let worker = temp.path().join("fake_faiss.py");
    fs::write(&worker, FAKE_FAISS_WORKER).expect("worker script");
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o755)).expect("worker permissions");
    let out = temp.path().join("faiss.json");
    let output = command()
        .args([
            "run",
            "--dataset",
            MINI,
            "--engine",
            "faiss",
            "--faiss-python",
            worker.to_str().expect("worker path"),
            "--out",
            out.to_str().expect("out path"),
        ])
        .output()
        .expect("faiss run");
    assert!(output.status.success(), "{}", stdout(&output));
    let report: Value =
        serde_json::from_slice(&fs::read(&out).expect("report json")).expect("parse report");
    assert_eq!(report["engine"]["name"], "faiss");
    assert_eq!(report["engine"]["version"], "fake-faiss-1");
    assert_eq!(report["engine"]["metric"], "squared_l2");
    assert_eq!(report["summary"]["cases"], 8);
    assert_eq!(report["summary"]["failed"], 0);
    assert_eq!(report["summary"]["errors"], 0);
}
