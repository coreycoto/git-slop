use std::fs;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

fn git(root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

fn fixture_repository() -> tempfile::TempDir {
    let root = tempdir().expect("repository");
    git(root.path(), &["init", "--quiet"]);
    git(root.path(), &["config", "user.name", "Fixture"]);
    git(
        root.path(),
        &["config", "user.email", "fixture@example.com"],
    );
    fs::create_dir_all(root.path().join("src")).expect("src");
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn value() -> usize { 1 }\n",
    )
    .expect("source");
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    root
}

fn write_report(repository: &std::path::Path, output: &std::path::Path) -> std::path::PathBuf {
    let state = tempdir().expect("state");
    cargo_bin_cmd!("git-slop")
        .current_dir(repository)
        .args([
            "find",
            "--quiet",
            "--no-cache",
            "--state-dir",
            state.path().to_str().unwrap(),
            "--output-dir",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
    output.join("latest/report.json")
}

#[test]
fn find_uses_ephemeral_state_without_repairing_adoption_files() {
    let repository = fixture_repository();
    fs::create_dir_all(repository.path().join(".slop")).expect("slop");
    let sentinel = "# repository-owned\n";
    fs::write(repository.path().join(".slop/.gitignore"), sentinel).expect("sentinel");
    let state = tempdir().expect("state");
    let output = tempdir().expect("output");
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "find",
            "--quiet",
            "--no-cache",
            "--state-dir",
            state.path().to_str().expect("state path"),
            "--output-dir",
            output.path().to_str().expect("output path"),
        ])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(repository.path().join(".slop/.gitignore")).unwrap(),
        sentinel
    );
    assert!(output.path().join("latest/report.json").is_file());
}

#[test]
fn schema_four_validation_requires_explicit_legacy_acceptance() {
    let fixture = "tests/fixtures/reports/local_repo_folder_report.json";
    cargo_bin_cmd!("git-slop")
        .args(["report", "validate", fixture])
        .assert()
        .code(2);
    cargo_bin_cmd!("git-slop")
        .args(["report", "validate", fixture, "--allow-legacy"])
        .assert()
        .success();
}

#[test]
fn every_legacy_report_golden_migrates_to_the_current_contract() {
    let output = tempdir().unwrap();
    let fixtures = fs::read_dir("tests/fixtures/reports").unwrap();
    let mut migrated = 0usize;
    for entry in fixtures.filter_map(Result::ok) {
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_report.json"))
        {
            continue;
        }
        let source: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        if source["schema_version"] != 4 {
            continue;
        }
        let destination = output.path().join(path.file_name().unwrap());
        cargo_bin_cmd!("git-slop")
            .args([
                "report",
                "migrate",
                path.to_str().unwrap(),
                "--output",
                destination.to_str().unwrap(),
            ])
            .assert()
            .success();
        cargo_bin_cmd!("git-slop")
            .args(["report", "validate", destination.to_str().unwrap()])
            .assert()
            .success();
        migrated += 1;
    }
    assert!(
        migrated >= 5,
        "expected the complete schema-4 golden corpus"
    );
}

#[test]
fn schema_five_reports_reject_each_missing_required_root_field_with_a_pointer() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--no-cache"])
        .assert()
        .success();
    let report_path = repository.path().join(".slop/latest/report.json");
    let report: Value = serde_json::from_str(&fs::read_to_string(&report_path).unwrap()).unwrap();
    cargo_bin_cmd!("git-slop")
        .args(["report", "validate", report_path.to_str().unwrap()])
        .assert()
        .success();
    let schema_output = cargo_bin_cmd!("git-slop")
        .args(["schema", "report"])
        .output()
        .expect("schema");
    let schema: Value = serde_json::from_slice(&schema_output.stdout).unwrap();
    for key in schema["required"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        let mut mutated = report.clone();
        mutated.as_object_mut().unwrap().remove(key);
        let path = repository.path().join(format!("missing-{key}.json"));
        fs::write(&path, serde_json::to_vec(&mutated).unwrap()).unwrap();
        let output = cargo_bin_cmd!("git-slop")
            .args(["report", "validate", path.to_str().unwrap()])
            .output()
            .expect("validate");
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("required_field_missing"), "{stderr}");
        assert!(stderr.contains(&format!("/{key}")), "{stderr}");
    }
}

#[test]
fn schema_five_rejects_every_missing_file_field_and_unknown_fields() {
    let repository = fixture_repository();
    let output = tempdir().expect("output");
    let report_path = write_report(repository.path(), output.path());
    let report: Value = serde_json::from_str(&fs::read_to_string(report_path).unwrap()).unwrap();
    let schema_output = cargo_bin_cmd!("git-slop")
        .args(["schema", "report"])
        .output()
        .expect("schema");
    assert!(schema_output.status.success());
    let schema: Value = serde_json::from_slice(&schema_output.stdout).unwrap();
    for key in schema["$defs"]["file"]["required"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        let mut mutated = report.clone();
        mutated["files"][0].as_object_mut().unwrap().remove(key);
        let path = repository.path().join(format!("missing-file-{key}.json"));
        fs::write(&path, serde_json::to_vec(&mutated).unwrap()).unwrap();
        let output = cargo_bin_cmd!("git-slop")
            .args(["report", "validate", path.to_str().unwrap()])
            .output()
            .expect("validate");
        assert_eq!(output.status.code(), Some(2), "field {key}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("/files/0/{key}")),
            "{key}: {stderr}"
        );
    }
    for pointer in ["root", "file"] {
        let mut mutated = report.clone();
        if pointer == "root" {
            mutated["unexpected"] = Value::Bool(true);
        } else {
            mutated["files"][0]["unexpected"] = Value::Bool(true);
        }
        let path = repository.path().join(format!("unknown-{pointer}.json"));
        fs::write(&path, serde_json::to_vec(&mutated).unwrap()).unwrap();
        cargo_bin_cmd!("git-slop")
            .args(["report", "validate", path.to_str().unwrap()])
            .assert()
            .code(2);
    }
}

#[test]
fn schema_five_rejects_unknown_nested_cost_diagnostic_and_relationship_fields() {
    let repository = fixture_repository();
    let output = tempdir().expect("output");
    let report_path = write_report(repository.path(), output.path());
    let report: Value = serde_json::from_str(&fs::read_to_string(report_path).unwrap()).unwrap();
    let mutations = [
        ("cost", "/files/0/costs/load/unexpected"),
        ("diagnostic", "/diagnostics/analysis/unexpected"),
        (
            "relationship",
            "/overlays/organization_health/relationships/duplicate_neighborhoods/0/unexpected",
        ),
    ];
    for (name, pointer) in mutations {
        let mut mutated = report.clone();
        if name == "relationship" {
            mutated
                .pointer_mut("/overlays/organization_health/relationships/duplicate_neighborhoods")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({
                    "id": "duplicate-neighborhood-test",
                    "kind": "duplicate_neighborhood",
                    "source_path": "src/lib.rs",
                    "target_path": "src/lib.rs",
                    "evidence_score": 1.0,
                    "unexpected": true
                }));
        } else {
            let parent = pointer.rsplit_once('/').unwrap().0;
            mutated
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".to_string(), Value::Bool(true));
        }
        let path = repository
            .path()
            .join(format!("unknown-nested-{name}.json"));
        fs::write(&path, serde_json::to_vec(&mutated).unwrap()).unwrap();
        let validation = cargo_bin_cmd!("git-slop")
            .args(["report", "validate", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(validation.status.code(), Some(2), "{name}");
        assert!(String::from_utf8_lossy(&validation.stderr).contains("unknown_field"));
    }
}

#[test]
fn comparison_allows_file_additions_and_detects_equal_metric_content_changes() {
    let repository = fixture_repository();
    let base_output = tempdir().expect("base output");
    let base_path = write_report(repository.path(), base_output.path());
    fs::write(
        repository.path().join("src/added.rs"),
        "pub fn added() {}\n",
    )
    .unwrap();
    git(repository.path(), &["add", "."]);
    git(
        repository.path(),
        &["commit", "--quiet", "-m", "add healthy file"],
    );
    let head_output = tempdir().expect("head output");
    let head_path = write_report(repository.path(), head_output.path());
    let comparison = cargo_bin_cmd!("git-slop")
        .args([
            "compare",
            "--base",
            base_path.to_str().unwrap(),
            "--head",
            head_path.to_str().unwrap(),
            "--format",
            "json",
            "--detail",
            "full",
        ])
        .output()
        .expect("compare");
    assert!(
        comparison.status.success(),
        "{}",
        String::from_utf8_lossy(&comparison.stderr)
    );
    let comparison: Value = serde_json::from_slice(&comparison.stdout).unwrap();
    assert_eq!(comparison["summary"]["files"]["added"], 1);

    let base: Value = serde_json::from_str(&fs::read_to_string(base_path).unwrap()).unwrap();
    let mut changed = base.clone();
    changed["files"][0]["content_fingerprint"] = Value::String("f".repeat(64));
    changed["compare_index"]["files"][0]["content_fingerprint"] = Value::String("f".repeat(64));
    let changed_path = repository.path().join("equal-metrics-changed-content.json");
    fs::write(&changed_path, serde_json::to_vec(&changed).unwrap()).unwrap();
    let comparison = cargo_bin_cmd!("git-slop")
        .args([
            "compare",
            "--base",
            base_output
                .path()
                .join("latest/report.json")
                .to_str()
                .unwrap(),
            "--head",
            changed_path.to_str().unwrap(),
            "--format",
            "json",
            "--detail",
            "full",
        ])
        .output()
        .expect("compare");
    assert!(
        comparison.status.success(),
        "{}",
        String::from_utf8_lossy(&comparison.stderr)
    );
    let comparison: Value = serde_json::from_slice(&comparison.stdout).unwrap();
    assert_eq!(comparison["file_deltas"][0]["content_status"], "changed");
    assert_eq!(comparison["file_deltas"][0]["metric_status"], "unchanged");
    assert_eq!(comparison["file_deltas"][0]["status"], "source_changed");
}

#[test]
fn unrelated_no_remote_repositories_are_incompatible_and_local_remotes_are_redacted() {
    let first = fixture_repository();
    let second = fixture_repository();
    fs::write(
        second.path().join("src/lib.rs"),
        "pub fn other() -> usize { 2 }\n",
    )
    .unwrap();
    git(second.path(), &["add", "."]);
    git(
        second.path(),
        &[
            "commit",
            "--quiet",
            "--amend",
            "-m",
            "different root history",
        ],
    );
    git(
        first.path(),
        &[
            "remote",
            "add",
            "origin",
            "file:///private/workspace/secret.git",
        ],
    );
    let first_output = tempdir().unwrap();
    let second_output = tempdir().unwrap();
    let first_report = write_report(first.path(), first_output.path());
    let second_report = write_report(second.path(), second_output.path());
    let report: Value = serde_json::from_str(&fs::read_to_string(&first_report).unwrap()).unwrap();
    let remote = report["repo"]["remote_url"].as_str().unwrap();
    assert!(remote.starts_with("local:sha256:"), "{remote}");
    assert!(!remote.contains("workspace"));
    cargo_bin_cmd!("git-slop")
        .args([
            "compare",
            "--base",
            first_report.to_str().unwrap(),
            "--head",
            second_report.to_str().unwrap(),
        ])
        .assert()
        .code(2);
}

#[test]
fn doctor_json_bundle_keeps_stdout_as_one_json_document() {
    let repository = fixture_repository();
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["doctor", "--format", "json", "--bundle", "diagnostic.json"])
        .output()
        .expect("doctor");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("stdout is one JSON value");
    assert!(
        payload["bundle_path"]
            .as_str()
            .unwrap()
            .ends_with("diagnostic.json")
    );
    assert!(repository.path().join("diagnostic.json").is_file());
}

#[test]
fn prune_dry_run_enforces_count_and_byte_limits_as_json() {
    let repository = fixture_repository();
    let runs = repository.path().join(".slop/runs");
    for (name, bytes) in [
        ("2026-08-09T00-00-03Z", 30usize),
        ("2026-08-09T00-00-02Z", 20usize),
        ("2026-08-09T00-00-01Z", 10usize),
    ] {
        let path = runs.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("report.json"), vec![b'x'; bytes]).unwrap();
    }
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "prune",
            "--keep",
            "3",
            "--max-bytes",
            "45",
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .expect("prune");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["before"]["runs"], 3);
    assert_eq!(payload["before"]["bytes"], 60);
    assert_eq!(payload["after"]["runs"], 1);
    assert_eq!(payload["after"]["bytes"], 30);
    assert_eq!(payload["removed"]["runs"], 2);
    assert!(runs.join("2026-08-09T00-00-01Z").is_dir());
}

#[test]
fn unborn_repository_reports_not_applicable_history() {
    let repository = tempdir().unwrap();
    git(repository.path(), &["init", "--quiet"]);
    let output = tempdir().unwrap();
    let state = tempdir().unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "find",
            "--quiet",
            "--allow-empty-scope",
            "--no-cache",
            "--state-dir",
            state.path().to_str().unwrap(),
            "--output-dir",
            output.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    let report: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("latest/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report["diagnostics"]["analysis"]["history"]["history_status"],
        "not_applicable_unborn_repository"
    );
    assert_eq!(
        report["evidence_completeness"]["history"],
        "not_applicable_unborn_repository"
    );
    assert_eq!(
        report["evidence_completeness"]["churn_window"],
        "not_applicable"
    );
    assert_eq!(
        report["evidence_completeness"]["author_evidence"],
        "not_applicable"
    );
}

#[cfg(unix)]
#[test]
fn tracked_broken_symlink_is_a_valid_scope() {
    use std::os::unix::fs::symlink;

    let repository = fixture_repository();
    symlink("missing-target", repository.path().join("broken-link")).unwrap();
    git(repository.path(), &["add", "broken-link"]);
    git(
        repository.path(),
        &["commit", "--quiet", "-m", "add broken link"],
    );
    let output = tempdir().unwrap();
    let state = tempdir().unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "find",
            "--quiet",
            "--scope",
            "broken-link",
            "--no-cache",
            "--state-dir",
            state.path().to_str().unwrap(),
            "--output-dir",
            output.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    let report: Value = serde_json::from_str(
        &fs::read_to_string(output.path().join("latest/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["files"][0]["path"], "broken-link");
    assert_eq!(report["files"][0]["analysis_status"], "analyzed");
    assert!(report["files"][0]["symlink_metadata"].is_object());
}

#[test]
fn policy_checks_accept_non_text_inventory_but_reject_real_coverage_loss() {
    let repository = fixture_repository();
    fs::create_dir_all(repository.path().join("assets")).unwrap();
    fs::write(
        repository.path().join("assets/image.png"),
        b"\x89PNG\r\n\x1a\n\0binary",
    )
    .unwrap();
    git(repository.path(), &["add", "assets/image.png"]);
    git(
        repository.path(),
        &["commit", "--quiet", "-m", "add binary asset"],
    );

    let output = tempdir().unwrap();
    let report_path = write_report(repository.path(), output.path());
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["check", "--report", report_path.to_str().unwrap()])
        .assert()
        .success();

    let mut report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).unwrap()).unwrap();
    let binary = report["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["path"] == "assets/image.png")
        .unwrap();
    assert_eq!(binary["analysis_status"], "skipped");
    assert_eq!(binary["skipped_reason"], "binary");
    binary["skipped_reason"] = Value::String("large_file_limit".into());
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["check", "--report", report_path.to_str().unwrap()])
        .assert()
        .code(2);
}

#[test]
fn config_schema_uses_the_published_schemas_path() {
    let output = cargo_bin_cmd!("git-slop")
        .args(["schema", "config"])
        .output()
        .expect("schema");
    assert!(output.status.success());
    let schema: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(
        schema["$id"],
        "https://github.com/coreycoto/git-slop/blob/v0.11.5/schemas/config-2.json"
    );
}
