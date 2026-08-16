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
    let mut report: Value =
        serde_json::from_slice(&fs::read(fixture(name)).expect("read report fixture"))
            .expect("parse report fixture");
    let content_sha256 = if name == "compare_head_report.json" {
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    } else {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    };
    report["repo"]["head_sha"] = if name == "compare_head_report.json" {
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    } else {
        json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    };
    let files = report["files"].as_array_mut().expect("fixture files");
    for file in files.iter_mut() {
        file["analysis_status"] = json!("analyzed");
        file["content_fingerprint"] = json!(content_sha256);
        file["content_sha256"] = json!(content_sha256);
    }
    let file_count = files.len();
    let folder_count = report["folders"].as_array().map_or(0, std::vec::Vec::len);
    report["diagnostics"] = json!({
        "analysis": {"analysis_status": "complete"}
    });
    report["repo"]["worktree_clean"] = json!(true);
    report["collection_metadata"] = json!({
        "files": {
            "total": file_count,
            "returned": file_count,
            "limit": null,
            "truncated": false
        },
        "folders": {
            "total": folder_count,
            "returned": folder_count,
            "limit": null,
            "truncated": false
        }
    });
    report
}

fn complete_fixture(directory: &TempDir, name: &str) -> PathBuf {
    write_report(directory, name, &load_fixture(name))
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

include!("report_export_contracts/group_1.rs");
include!("report_export_contracts/group_2.rs");
include!("report_export_contracts/group_3.rs");
