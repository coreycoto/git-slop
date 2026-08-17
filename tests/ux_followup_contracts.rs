use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/reports").join(name)
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read JSON file")).expect("parse JSON file")
}

#[test]
fn html_export_is_responsive_and_accessible() {
    let temporary = TempDir::new().expect("temporary directory");
    let output = temporary.path().join("report.html");
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args(["html", "--report"])
        .arg(fixture("relationship_focused_report.json"))
        .arg("--output")
        .arg(&output)
        .assert()
        .success();
    let html = fs::read_to_string(output).expect("HTML report");
    assert!(html.contains("<main id=\"report-main\""));
    assert!(html.contains("class=\"table-shell\""));
    assert!(html.contains("class=\"overview-grid\""));
    assert!(html.contains("class=\"explorer-layout\""));
    assert!(html.contains("aria-label=\"Scrollable report table\""));
    assert!(html.contains("<label for=\"query\">Search records</label>"));
    assert!(html.contains("<th scope=\"col\" aria-sort="));
    assert!(html.contains("const sortDefaults = {"));
    assert!(html.contains("queue: { key: \"__rank\", ascending: true }"));
    assert!(html.contains("observations: { key: \"__rank\", ascending: true }"));
    assert!(html.contains("<label for=\"context-band\">Context/load band</label>"));
    assert!(html.contains("<label for=\"slop-band\">Maintenance band</label>"));
    assert!(html.contains("<label for=\"severity\">Review severity</label>"));
    assert!(html.contains("id=\"page-number\""));
    assert!(html.contains("id=\"page-size\""));
    assert!(html.contains("function humanizeCode(value)"));
    assert!(html.contains("old_file: \"Older file\""));
    assert!(html.contains("old_and_volatile: \"Older file with sustained churn\""));
    assert!(html.contains("data-copy="));
    assert!(html.contains("function recordIdentity(recordView, record)"));
    assert!(html.contains("function clearSelection("));
    assert!(html.contains("@media (max-width: 520px)"));
    assert!(html.contains("overflow-x: auto"));
}

#[test]
fn html_serve_is_loopback_only_and_returns_the_portable_report() {
    let temporary = TempDir::new().expect("temporary directory");
    let output = temporary.path().join("report.html");
    let mut child = Command::new(assert_cmd::cargo::cargo_bin("git-slop"))
        .current_dir(manifest_dir())
        .args(["html", "--report"])
        .arg(fixture("relationship_focused_report.json"))
        .arg("--output")
        .arg(&output)
        .args(["--serve", "--serve-seconds", "1"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("start temporary report server");
    let mut stdout = child.stdout.take().expect("server stdout");
    let started = Instant::now();
    let mut captured = String::new();
    let address = loop {
        let mut byte = [0_u8; 1];
        if stdout.read(&mut byte).expect("read server output") == 0 {
            panic!("server exited before advertising its address: {captured}");
        }
        captured.push(byte[0] as char);
        if let Some(start) = captured.find("http://127.0.0.1:") {
            if let Some(end) = captured[start..].find("/\n") {
                break captured[start + "http://".len()..start + end].to_string();
            }
        }
        assert!(started.elapsed() < Duration::from_secs(2));
    };
    let mut stream = TcpStream::connect(address).expect("connect to loopback report server");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .expect("request report");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read report");
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
    assert!(response.contains("Content-Security-Policy: default-src 'none'"));
    assert!(response.contains("<!doctype html>"));
    assert!(
        child
            .wait()
            .expect("wait for temporary report server")
            .success()
    );

    cargo_bin_cmd!("git-slop")
        .args(["html", "--report"])
        .arg(fixture("relationship_focused_report.json"))
        .arg("--open")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--serve"));
}

#[test]
fn root_invocation_is_successful_onboarding_help() {
    cargo_bin_cmd!("git-slop")
        .assert()
        .success()
        .stdout(predicate::str::contains("QUICK START"))
        .stdout(predicate::str::contains("git slop list interventions"));
}

#[test]
fn list_help_names_all_decision_surfaces() {
    cargo_bin_cmd!("git-slop")
        .args(["list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("policy-failures"))
        .stdout(predicate::str::contains("interventions"))
        .stdout(predicate::str::contains("observations"))
        .stdout(predicate::str::contains("health-findings"));
}

#[test]
fn list_help_only_advertises_supported_filters() {
    let profiles = cargo_bin_cmd!("git-slop")
        .args(["list", "profiles", "--help"])
        .output()
        .expect("profiles help");
    assert!(profiles.status.success());
    let profiles = String::from_utf8(profiles.stdout).unwrap();
    for unsupported in ["--path", "--language", "--classification", "--severity"] {
        assert!(
            !profiles.contains(unsupported),
            "{unsupported} in {profiles}"
        );
    }
    assert!(profiles.contains("--profile"));

    let relationships = cargo_bin_cmd!("git-slop")
        .args(["list", "relationships", "--help"])
        .output()
        .expect("relationships help");
    assert!(relationships.status.success());
    let relationships = String::from_utf8(relationships.stdout).unwrap();
    assert!(relationships.contains("--path"));
    assert!(!relationships.contains("--severity"));
}

#[test]
fn list_relationships_and_clusters_are_ranked_and_clusters_are_unique() {
    let report = fixture("local_repo_folder_report.json");
    let relationships = cargo_bin_cmd!("git-slop")
        .args(["list", "relationships", "--report"])
        .arg(&report)
        .args(["--format", "json"])
        .output()
        .expect("relationships");
    assert!(relationships.status.success());
    let relationships: Value = serde_json::from_slice(&relationships.stdout).unwrap();
    let scores = relationships["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["evidence_score"].as_f64().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "{scores:?}"
    );

    let clusters = cargo_bin_cmd!("git-slop")
        .args(["list", "clusters", "--report"])
        .arg(&report)
        .args(["--format", "json"])
        .output()
        .expect("clusters");
    assert!(clusters.status.success());
    let clusters: Value = serde_json::from_slice(&clusters.stdout).unwrap();
    assert_eq!(clusters["collection"]["total"], 4);
    let items = clusters["items"].as_array().unwrap();
    let ids = items
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), items.len());
    let scores = items
        .iter()
        .map(|item| item["evidence_score"].as_f64().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "{scores:?}"
    );
}

#[test]
fn list_profiles_renders_profile_totals_in_human_output() {
    let temporary = TempDir::new().unwrap();
    let report_path = temporary.path().join("report.json");
    let mut report = read_json(&fixture("local_repo_folder_report.json"));
    report["health"]["profile_rollups"] = serde_json::json!([
        {"name":"agent_context","totals":{"files":3,"tokens":1200,"lines":300,"code":250,"comments":20,"blanks":30}}
    ]);
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    cargo_bin_cmd!("git-slop")
        .args(["list", "profiles", "--report"])
        .arg(&report_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("PROFILE"))
        .stdout(predicate::str::contains("FILES"))
        .stdout(predicate::str::contains("TOKENS"))
        .stdout(predicate::str::contains("agent_context"));
}

#[test]
fn explain_distinguishes_low_support_and_concisely_reports_no_hotspots() {
    let temporary = TempDir::new().unwrap();
    let report_path = temporary.path().join("report.json");
    let mut report = read_json(&fixture("local_repo_folder_report.json"));
    report["evidence_completeness"] = serde_json::json!({
        "history":"complete",
        "repository_size":"low_support"
    });
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    cargo_bin_cmd!("git-slop")
        .args(["explain", "--report"])
        .arg(&report_path)
        .args(["--path", "src/git_slop/organization.py", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"low_support\""))
        .stdout(predicate::str::contains("\"incomplete\": false"));

    report["action_queue"] = serde_json::json!([]);
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    cargo_bin_cmd!("git-slop")
        .args(["explain", "--report"])
        .arg(&report_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Explain: no matching hotspots"))
        .stdout(predicate::str::contains("No action-queue hotspots"))
        .stdout(predicate::str::contains("Report and Evidence Provenance").not());
}

#[test]
fn generated_reference_uses_complete_command_paths() {
    let output = cargo_bin_cmd!("git-slop")
        .arg("reference")
        .output()
        .expect("generated reference");
    assert!(output.status.success());
    let reference = String::from_utf8(output.stdout).expect("UTF-8 reference");
    assert!(reference.contains("Usage: git-slop cache prune"));
    assert!(reference.contains("Usage: git-slop baseline ensure"));
    assert!(reference.contains("git slop advise --top 1"));
    assert!(!reference.contains("--evaluation-scenario"));
}

#[test]
fn generated_reference_matches_the_complete_runtime_exit_contract() {
    cargo_bin_cmd!("git-slop")
        .arg("reference")
        .assert()
        .success()
        .stdout(predicate::str::contains("`2`: command usage"))
        .stdout(predicate::str::contains("`3`: repository access"))
        .stdout(predicate::str::contains(
            "`4`: a configured or measured resource limit",
        ));
}

#[test]
fn repository_errors_are_human_and_actionable() {
    let temporary = TempDir::new().unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(temporary.path())
        .arg("doctor")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Not inside a Git repository"))
        .stderr(predicate::str::contains("--repo <PATH>"))
        .stderr(predicate::str::contains("git rev-parse").not());
}
