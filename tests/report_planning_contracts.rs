use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};

const HISTORICAL_FOLDER_PATH: &str = "src/git_slop";
const HISTORICAL_FILE_PATH: &str = "src/git_slop/organization.py";
const RELATIONSHIP_ID: &str = "near_duplicate_neighborhood-35e7fad1c4e0";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/reports").join(name)
}

fn run_cli(args: &[&str]) -> Output {
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args(args)
        .output()
        .expect("run git-slop")
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "git-slop failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git-slop stdout is UTF-8")
        .replace("\r\n", "\n")
}

fn json_stdout(output: Output) -> Value {
    serde_json::from_str(&stdout(output)).expect("git-slop stdout is JSON")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read JSON fixture")).expect("parse JSON fixture")
}

fn assert_fixture_unchanged(path: &Path, original: &[u8]) {
    assert_eq!(
        fs::read(path).expect("reread report fixture"),
        original,
        "report consumers must not rewrite {}",
        path.display()
    );
}

#[test]
fn local_folder_explain_matches_the_native_text_golden() {
    let report = fixture("local_repo_folder_report.json");
    let original_report = fs::read(&report).expect("read report fixture");
    let expected = fs::read_to_string(fixture("local_repo_folder_explain.txt"))
        .expect("read folder explain golden")
        .replace("\r\n", "\n");
    let report_path = report.to_str().expect("report fixture path");

    let actual = stdout(run_cli(&[
        "explain",
        "--report",
        report_path,
        "--path",
        HISTORICAL_FOLDER_PATH,
    ]));

    assert_eq!(actual, expected);
    assert_fixture_unchanged(&report, &original_report);
}

#[test]
fn explain_path_selector_distinguishes_historical_file_and_folder_records() {
    let report = fixture("local_repo_folder_report.json");
    let original_report = fs::read(&report).expect("read report fixture");
    let report_path = report.to_str().expect("report fixture path");

    // These are opaque paths captured in the historical report fixture. They do
    // not refer to source files that must exist in the current Rust checkout.
    let file_payload = json_stdout(run_cli(&[
        "explain",
        "--report",
        report_path,
        "--path",
        HISTORICAL_FILE_PATH,
        "--format",
        "json",
    ]));
    let folder_payload = json_stdout(run_cli(&[
        "explain",
        "--report",
        report_path,
        "--path",
        HISTORICAL_FOLDER_PATH,
        "--format",
        "json",
    ]));

    assert_eq!(file_payload["schema_version"], 2);
    assert_eq!(file_payload["report_schema_version"], 4);
    assert_eq!(
        file_payload["selector"],
        json!({"kind": "path", "value": HISTORICAL_FILE_PATH})
    );
    assert_eq!(file_payload["target"]["path"], HISTORICAL_FILE_PATH);
    assert_eq!(file_payload["target"]["record_type"], "file");

    assert_eq!(folder_payload["schema_version"], 2);
    assert_eq!(folder_payload["report_schema_version"], 4);
    assert_eq!(
        folder_payload["selector"],
        json!({"kind": "path", "value": HISTORICAL_FOLDER_PATH})
    );
    assert_eq!(folder_payload["target"]["path"], HISTORICAL_FOLDER_PATH);
    assert_eq!(folder_payload["target"]["record_type"], "folder");
    assert_eq!(
        folder_payload["cost_summary"]["descendant_hotspots"]
            .as_array()
            .map(Vec::len),
        Some(5)
    );
    assert!(
        folder_payload["overlay_summary"]
            .get("descendant_overlay_maxima")
            .is_some()
    );
    assert!(
        folder_payload["evidence_summary"]
            .get("strongest_overlays")
            .is_some()
    );

    for key in ["supporting_relationships", "supporting_clusters"] {
        let ids: Vec<&str> = folder_payload[key]
            .as_array()
            .expect("supporting evidence array")
            .iter()
            .map(|item| item["id"].as_str().expect("supporting evidence id"))
            .collect();
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate IDs in {key}");
    }

    assert_fixture_unchanged(&report, &original_report);
}

#[test]
fn large_repo_top_explain_matches_the_text_golden_and_defaults_to_five() {
    let report = fixture("large_repo_top_report.json");
    let original_report = fs::read(&report).expect("read report fixture");
    let expected = fs::read_to_string(fixture("large_repo_top_explain.txt"))
        .expect("read top explain golden")
        .replace("\r\n", "\n");
    let report_path = report.to_str().expect("report fixture path");

    let actual = stdout(run_cli(&["explain", "--report", report_path, "--top", "5"]));
    assert_eq!(actual, expected);
    assert_eq!(actual.matches("Interpretation boundary").count(), 1);

    let report_payload = read_json(&report);
    for (index, item) in report_payload["action_queue"]
        .as_array()
        .expect("action queue")
        .iter()
        .take(5)
        .enumerate()
    {
        let path = item["path"].as_str().expect("action queue path");
        assert!(actual.contains(&format!("{}. {path}", index + 1)));
    }

    let default_payload = json_stdout(run_cli(&[
        "explain",
        "--report",
        report_path,
        "--format",
        "json",
    ]));
    assert_eq!(default_payload["schema_version"], 2);
    assert_eq!(default_payload["report_schema_version"], 4);
    assert_eq!(
        default_payload["selector"],
        json!({"kind": "top", "value": 5})
    );
    assert_eq!(
        default_payload["target"],
        json!({"kind": "top", "count": 5})
    );
    assert_eq!(default_payload["items"].as_array().map(Vec::len), Some(5));
    assert_fixture_unchanged(&report, &original_report);
}

#[test]
fn folder_plan_json_is_deterministic_bounded_and_preview_only() {
    let report = fixture("local_repo_folder_report.json");
    let original_report = fs::read(&report).expect("read report fixture");
    let report_path = report.to_str().expect("report fixture path");
    let args = [
        "plan",
        "--report",
        report_path,
        "--path",
        HISTORICAL_FOLDER_PATH,
        "--format",
        "json",
    ];

    let first = stdout(run_cli(&args));
    let second = stdout(run_cli(&args));
    assert_eq!(first, second);

    let payload: Value = serde_json::from_str(&first).expect("plan JSON");
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(payload["report_schema_version"], 4);
    assert_eq!(payload["command"], "plan");
    assert_eq!(
        payload["selector"],
        json!({"kind": "path", "value": HISTORICAL_FOLDER_PATH})
    );
    assert_eq!(payload["target"]["record_type"], "folder");
    assert_eq!(payload["proposed_slices"].as_array().map(Vec::len), Some(3));

    let slices = payload["proposed_slices"]
        .as_array()
        .expect("proposed plan slices");
    assert!(slices.iter().all(|slice| {
        slice["scope_paths"]
            .as_array()
            .is_some_and(|paths| paths.len() <= 5)
    }));
    assert_eq!(
        slices[0]["scope_paths"],
        json!([
            "src/git_slop/organization.py",
            "src/git_slop/reporting.py",
            "src/git_slop/history.py",
            "src/git_slop/__init__.py",
            "src/git_slop/cli.py"
        ])
    );
    assert_eq!(
        payload["backlog_handoff"]["mutation_policy"],
        "preview_only"
    );
    assert_eq!(
        payload["backlog_handoff"]["target_plugin_skill"],
        "$project-management-workflows:plan-to-backlog-preview"
    );
    assert!(
        slices
            .iter()
            .all(|slice| { slice["backlog_handoff"]["mutation_policy"] == "preview_only" })
    );

    assert_fixture_unchanged(&report, &original_report);
}

#[test]
fn relationship_plan_matches_the_existing_text_and_json_goldens() {
    let report = fixture("relationship_focused_report.json");
    let original_report = fs::read(&report).expect("read report fixture");
    let report_path = report.to_str().expect("report fixture path");

    let expected_text = fs::read_to_string(fixture("relationship_focused_plan.txt"))
        .expect("read relationship plan text golden")
        .replace("\r\n", "\n");
    let actual_text = stdout(run_cli(&[
        "plan",
        "--report",
        report_path,
        "--relationship",
        RELATIONSHIP_ID,
    ]));
    assert_eq!(actual_text, expected_text);

    let expected_json = read_json(&fixture("relationship_focused_plan.json"));
    let actual_json = json_stdout(run_cli(&[
        "plan",
        "--report",
        report_path,
        "--relationship",
        RELATIONSHIP_ID,
        "--format",
        "json",
    ]));
    assert_eq!(actual_json, expected_json);
    assert_eq!(
        actual_json["proposed_slices"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(
        actual_json["proposed_slices"]
            .as_array()
            .expect("proposed plan slices")
            .iter()
            .all(|slice| {
                slice["scope_paths"]
                    .as_array()
                    .is_some_and(|paths| paths.len() <= 5)
            })
    );

    assert_fixture_unchanged(&report, &original_report);
}
