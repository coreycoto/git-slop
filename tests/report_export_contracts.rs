use std::fs;
use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::TempDir;

const COMPARE_BOUNDARY: &str = "Compare boundary: this is a read-only comparison of two existing reports. It does not rerun the detector, imply causality, mutate repo state, or change detector scoring semantics.";
const SARIF_BOUNDARY: &str = "SARIF export boundary: this is a deterministic projection of existing git-slop report evidence. It does not rerun the detector, upload results, mutate code, or change detector scoring semantics.";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/reports").join(name)
}

fn load_fixture(name: &str) -> Value {
    serde_json::from_slice(&fs::read(fixture(name)).expect("read report fixture"))
        .expect("parse report fixture")
}

fn write_report(directory: &TempDir, name: &str, report: &Value) -> PathBuf {
    let path = directory.path().join(name);
    fs::write(
        &path,
        serde_json::to_vec_pretty(report).expect("serialize temporary report"),
    )
    .expect("write temporary report");
    path
}

fn command() -> Command {
    let mut command = cargo_bin_cmd!("git-slop");
    command.current_dir(manifest_dir());
    command
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_exit_code(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "unexpected exit status\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn stdout_json(output: &Output) -> Value {
    assert_success(output);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout was not JSON: {error}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn item_by_path<'a>(items: &'a Value, path: &str) -> &'a Value {
    items
        .as_array()
        .expect("array payload")
        .iter()
        .find(|item| item["path"] == path)
        .unwrap_or_else(|| panic!("missing payload item for {path}"))
}

#[test]
fn compare_json_preserves_status_deltas_and_queue_movement() {
    let base = fixture("compare_base_report.json");
    let head = fixture("compare_head_report.json");
    let output = command()
        .args(["compare", "--base"])
        .arg(&base)
        .arg("--head")
        .arg(&head)
        .args(["--format", "json"])
        .output()
        .expect("run compare JSON command");
    let payload = stdout_json(&output);

    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["report_schema_version"], 4);
    assert_eq!(payload["command"], "compare");
    assert_eq!(payload["base_report"]["repo_name"], "compare-fixture");
    assert_eq!(payload["base_report"]["head_sha"], "base-sha");
    assert_eq!(payload["head_report"]["head_sha"], "head-sha");
    assert_eq!(payload["summary"]["files"]["added"], 1);
    assert_eq!(payload["summary"]["files"]["removed"], 1);
    assert_eq!(payload["summary"]["files"]["changed"], 2);
    assert_eq!(payload["summary"]["files"]["unchanged"], 0);
    assert_eq!(payload["summary"]["folders"]["changed"], 1);
    assert_eq!(payload["summary"]["worsened_file_count"], 1);
    assert_eq!(payload["summary"]["improved_file_count"], 1);

    let a = item_by_path(&payload["file_deltas"], "src/a.py");
    assert_eq!(a["status"], "changed");
    assert_eq!(a["slop_score_delta"], 50.0);
    assert_eq!(a["token_delta"], 60);
    assert_eq!(a["load_pressure_delta"], 0.5);
    assert_eq!(a["context_band_delta"], 2);
    assert_eq!(a["slop_band_delta"], 2);
    assert_eq!(a["overlay_deltas"][0]["label"], "verification");
    assert_eq!(a["overlay_deltas"][0]["delta"], 0.6);

    let b = item_by_path(&payload["file_deltas"], "src/b.py");
    assert_eq!(b["status"], "changed");
    assert_eq!(b["slop_score_delta"], -20.0);
    assert_eq!(b["token_delta"], -50);
    assert_eq!(b["context_band_delta"], -1);
    assert_eq!(b["slop_band_delta"], -1);
    assert_eq!(b["overlay_deltas"][0]["label"], "navigation");
    assert_eq!(b["overlay_deltas"][0]["delta"], -0.3);

    assert_eq!(
        item_by_path(&payload["file_deltas"], "src/new.py")["status"],
        "added"
    );
    assert_eq!(
        item_by_path(&payload["file_deltas"], "src/removed.py")["status"],
        "removed"
    );

    let new_queue = item_by_path(&payload["queue_movement"], "src/new.py");
    assert_eq!(new_queue["status"], "newly_queued");
    assert!(new_queue["base_position"].is_null());
    assert_eq!(new_queue["head_position"], 1);
    assert!(new_queue["position_delta"].is_null());
    let a_queue = item_by_path(&payload["queue_movement"], "src/a.py");
    assert_eq!(a_queue["status"], "moved_down");
    assert_eq!(a_queue["base_position"], 1);
    assert_eq!(a_queue["head_position"], 2);
    assert_eq!(a_queue["position_delta"], 1);
    let b_queue = item_by_path(&payload["queue_movement"], "src/b.py");
    assert_eq!(b_queue["status"], "moved_down");
    assert_eq!(b_queue["base_position"], 2);
    assert_eq!(b_queue["head_position"], 3);
    assert_eq!(b_queue["position_delta"], 1);

    assert_eq!(payload["overlay_deltas"][0]["label"], "verification");
    assert_eq!(payload["overlay_deltas"][0]["total_delta"], 0.6);
    assert_eq!(payload["overlay_deltas"][1]["label"], "navigation");
    assert_eq!(payload["overlay_deltas"][1]["total_delta"], -0.3);
    assert_eq!(payload["boundary_note"], COMPARE_BOUNDARY);
}

#[test]
fn compare_handles_sparse_and_null_overlay_evidence_without_inventing_deltas() {
    let directory = TempDir::new().expect("temporary report directory");
    let mut report = load_fixture("compare_base_report.json");
    let files = report["files"].as_array_mut().expect("fixture files");
    files[0]["overlays"] = json!({
        "organization_health": null,
        "verification": null,
        "navigation": {"navigation_pressure": 0.0}
    });
    files[1]["overlays"] = Value::Null;
    files[2]
        .as_object_mut()
        .expect("file record")
        .remove("overlays");
    let base = write_report(&directory, "sparse-base.json", &report);
    let head = write_report(&directory, "sparse-head.json", &report);

    let output = command()
        .args(["compare", "--base"])
        .arg(base)
        .arg("--head")
        .arg(head)
        .args(["--format", "json"])
        .output()
        .expect("run sparse compare command");
    let payload = stdout_json(&output);

    for delta in payload["file_deltas"].as_array().expect("file deltas") {
        assert_eq!(delta["status"], "unchanged", "unexpected delta: {delta}");
        assert_eq!(delta["overlay_deltas"], json!([]));
    }
    assert_eq!(payload["overlay_deltas"], json!([]));
}

#[test]
fn compare_text_surface_honors_top_and_explains_its_boundary() {
    let output = command()
        .args(["compare", "--base"])
        .arg(fixture("compare_base_report.json"))
        .arg("--head")
        .arg(fixture("compare_head_report.json"))
        .args(["--top", "2"])
        .output()
        .expect("run compare text command");
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");

    assert!(stdout.contains("Compare: compare_base_report.json -> compare_head_report.json"));
    assert!(stdout.contains("Summary\n- files: added=1, removed=1, changed=2, unchanged=0"));
    assert!(stdout.contains("Top Worsened Files"));
    assert!(stdout.contains("- src/a.py: 20.0 -> 70.0 (delta=50.0)"));
    assert!(stdout.contains("Top Improved Files"));
    assert!(stdout.contains("- src/b.py: 50.0 -> 30.0 (delta=-20.0)"));
    assert!(stdout.contains("Queue Movement"));
    assert!(stdout.contains("- src/new.py: newly_queued base=None head=1"));
    assert!(stdout.contains("- src/a.py: moved_down base=1 head=2"));
    assert!(!stdout.contains("- src/b.py: moved_down base=2 head=3"));
    assert!(stdout.contains(COMPARE_BOUNDARY));
}

#[test]
fn compare_reports_missing_invalid_and_incomplete_inputs_as_usage_errors() {
    let directory = TempDir::new().expect("temporary report directory");
    let missing = directory.path().join("missing.json");
    let mut invalid_report = load_fixture("compare_base_report.json");
    invalid_report["schema_version"] = json!(3);
    let invalid = write_report(&directory, "invalid-schema.json", &invalid_report);
    let valid_base = fixture("compare_base_report.json");
    let valid_head = fixture("compare_head_report.json");

    let missing_output = command()
        .args(["compare", "--base"])
        .arg(&valid_base)
        .arg("--head")
        .arg(&missing)
        .output()
        .expect("run compare with missing report");
    assert_exit_code(&missing_output, 2);
    assert!(
        String::from_utf8_lossy(&missing_output.stderr)
            .contains(&format!("Report not found: {}", missing.display()))
    );

    let invalid_output = command()
        .args(["compare", "--base"])
        .arg(invalid)
        .arg("--head")
        .arg(&valid_head)
        .output()
        .expect("run compare with invalid report");
    assert_exit_code(&invalid_output, 2);
    assert!(String::from_utf8_lossy(&invalid_output.stderr).contains("schema_version must be 4"));

    let incomplete_output = command()
        .args(["compare", "--base"])
        .arg(&valid_base)
        .output()
        .expect("run incomplete compare command");
    assert_exit_code(&incomplete_output, 2);
    let incomplete_stderr = String::from_utf8_lossy(&incomplete_output.stderr);
    assert!(incomplete_stderr.contains("--head <HEAD>"));
    assert!(incomplete_stderr.contains("Usage:"));

    let bad_top_output = command()
        .args(["compare", "--base"])
        .arg(valid_base)
        .arg("--head")
        .arg(valid_head)
        .args(["--top", "0"])
        .output()
        .expect("run compare with zero top");
    assert_exit_code(&bad_top_output, 2);
    assert!(
        String::from_utf8_lossy(&bad_top_output.stderr)
            .contains("--top must be greater than zero.")
    );
}

#[test]
fn regression_gate_ignores_a_new_healthy_file_and_records_forced_scope_mismatches() {
    let directory = TempDir::new().expect("temporary report directory");
    let mut base = load_fixture("compare_base_report.json");
    let mut head = base.clone();
    head["files"].as_array_mut().expect("files").push(json!({
        "path": "src/healthy.rs",
        "tokens": 100,
        "context_band": "compact",
        "slop_score": 2.0,
        "slop_band": "low",
        "reason_codes": [],
        "costs": {},
        "overlays": {}
    }));
    base["scope"] =
        json!({"mode":"scoped","path":"src","selected_path_count":3,"selected_path_digest":"aaa"});
    head["scope"] =
        json!({"mode":"scoped","path":"lib","selected_path_count":4,"selected_path_digest":"bbb"});
    let base_path = write_report(&directory, "base.json", &base);
    let head_path = write_report(&directory, "head.json", &head);

    cargo_bin_cmd!("git-slop")
        .args(["compare", "--base"])
        .arg(&base_path)
        .arg("--head")
        .arg(&head_path)
        .arg("--fail-on-regression")
        .assert()
        .code(2)
        .stderr(predicates::str::contains("analysis scope path"));

    let output = cargo_bin_cmd!("git-slop")
        .args(["compare", "--base"])
        .arg(&base_path)
        .arg("--head")
        .arg(&head_path)
        .args(["--force", "--fail-on-regression", "--format", "json"])
        .output()
        .expect("forced compare");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("comparison JSON");
    assert_eq!(payload["summary"]["regression_count"], 0);
    assert_eq!(payload["baseline_compatible"], false);
    assert!(
        payload["compatibility_mismatches"]
            .as_array()
            .is_some_and(|items| items.len() >= 2)
    );
}

#[test]
fn sarif_stdout_preserves_sarif_tool_finding_and_evidence_contracts() {
    let report_path = fixture("large_repo_top_report.json");
    let report = load_fixture("large_repo_top_report.json");
    let output = command()
        .args(["sarif", "--report"])
        .arg(&report_path)
        .args(["--top", "2"])
        .output()
        .expect("run SARIF command");
    let payload = stdout_json(&output);

    assert_eq!(
        payload["$schema"],
        "https://json.schemastore.org/sarif-2.1.0.json"
    );
    assert_eq!(payload["version"], "2.1.0");
    let run = &payload["runs"][0];
    let driver = &run["tool"]["driver"];
    assert_eq!(driver["name"], "git-slop");
    assert_eq!(
        driver["informationUri"],
        "https://github.com/coreycoto/git-slop"
    );
    assert_eq!(driver["rules"].as_array().map(Vec::len), Some(2));
    let rule = &driver["rules"][0];
    assert_eq!(rule["id"], "git-slop.context-budget");
    assert_eq!(rule["name"], "Git Slop context budget");
    assert_eq!(rule["properties"]["precision"], "medium");
    assert!(
        rule["properties"]["tags"]
            .as_array()
            .expect("rule tags")
            .contains(&json!("maintainability"))
    );
    assert_eq!(run["automationDetails"]["id"], "git-slop/sarif");
    assert_eq!(
        run["versionControlProvenance"][0]["repositoryUri"],
        "large-validation-repo"
    );

    let results = run["results"].as_array().expect("SARIF results");
    assert_eq!(results.len(), 2);
    for (index, result) in results.iter().enumerate() {
        let expected_path = &report["action_queue"][index]["path"];
        assert!(matches!(
            result["ruleId"].as_str(),
            Some("git-slop.context-budget" | "git-slop.maintenance-pressure")
        ));
        assert_eq!(result["level"], "warning");
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            *expected_path
        );
        assert_eq!(result["properties"]["git_slop"]["rank"], index + 1);
        assert!(result["properties"]["git_slop"]["costs"].is_object());
        assert!(result["properties"]["git_slop"]["strongest_overlays"].is_object());
    }
    let first_evidence = &results[0]["properties"]["git_slop"];
    assert_eq!(first_evidence["costs"], report["files"][0]["costs"]);
    assert_eq!(first_evidence["strongest_overlays"]["semantic_drift"], 1.0);
    assert_eq!(
        first_evidence["strongest_overlays"]["blast_radius"],
        0.651036
    );
    assert!(
        first_evidence["evidence_boundary"]
            .as_str()
            .expect("evidence boundary")
            .contains("does not rescore")
    );
    assert_eq!(
        run["invocations"][0]["properties"]["git_slop"]["schema_version"],
        1
    );
    assert_eq!(
        run["invocations"][0]["properties"]["git_slop"]["report_schema_version"],
        4
    );
    assert_eq!(
        run["invocations"][0]["properties"]["git_slop"]["boundary_note"],
        SARIF_BOUNDARY
    );
    assert_eq!(
        run["properties"]["git_slop"]["boundary_note"],
        SARIF_BOUNDARY
    );
}

#[test]
fn sarif_writes_the_same_contract_to_a_requested_output_file() {
    let directory = TempDir::new().expect("temporary SARIF directory");
    let output_path = directory.path().join("nested/git-slop.sarif");
    let output = command()
        .args(["sarif", "--report"])
        .arg(fixture("large_repo_top_report.json"))
        .args(["--top", "1", "--output"])
        .arg(&output_path)
        .output()
        .expect("run SARIF file export");
    assert_success(&output);
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains(&format!("Wrote SARIF report to {}.", output_path.display()))
    );
    let payload: Value =
        serde_json::from_slice(&fs::read(&output_path).expect("read SARIF output"))
            .expect("parse SARIF output");
    assert_eq!(payload["version"], "2.1.0");
    assert_eq!(
        payload["runs"][0]["results"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        payload["runs"][0]["results"][0]["properties"]["git_slop"]["rank"],
        1
    );
}

#[test]
fn sarif_reports_missing_invalid_and_zero_top_inputs_as_usage_errors() {
    let directory = TempDir::new().expect("temporary report directory");
    let missing = directory.path().join("missing.json");
    let mut invalid_report = load_fixture("large_repo_top_report.json");
    invalid_report["schema_version"] = json!(2);
    let invalid = write_report(&directory, "invalid-schema.json", &invalid_report);
    let valid = fixture("large_repo_top_report.json");

    let missing_output = command()
        .args(["sarif", "--report"])
        .arg(&missing)
        .output()
        .expect("run SARIF with missing report");
    assert_exit_code(&missing_output, 2);
    assert!(
        String::from_utf8_lossy(&missing_output.stderr)
            .contains(&format!("Report not found: {}", missing.display()))
    );

    let invalid_output = command()
        .args(["sarif", "--report"])
        .arg(invalid)
        .output()
        .expect("run SARIF with invalid report");
    assert_exit_code(&invalid_output, 2);
    assert!(String::from_utf8_lossy(&invalid_output.stderr).contains("schema_version must be 4"));

    let bad_top_output = command()
        .args(["sarif", "--report"])
        .arg(valid)
        .args(["--top", "0"])
        .output()
        .expect("run SARIF with zero top");
    assert_exit_code(&bad_top_output, 2);
    assert!(
        String::from_utf8_lossy(&bad_top_output.stderr)
            .contains("--top must be greater than zero.")
    );
}
