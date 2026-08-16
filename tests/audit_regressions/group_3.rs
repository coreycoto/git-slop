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
        "https://github.com/coreycoto/git-slop/blob/v0.11.6/schemas/config-2.json"
    );
}

#[test]
fn baseline_ensure_is_idempotent_and_fails_closed_on_drift() {
    let repository = fixture_repository();
    let output = tempdir().expect("report output");
    let report_path = write_report(repository.path(), output.path());
    let ensure = || {
        cargo_bin_cmd!("git-slop")
            .current_dir(repository.path())
            .args([
                "--error-format",
                "json",
                "baseline",
                "ensure",
                "--name",
                "audit",
                "--report",
                report_path.to_str().unwrap(),
                "--format",
                "json",
            ])
            .output()
            .expect("ensure baseline")
    };
    let created = ensure();
    assert!(created.status.success());
    let created: Value = serde_json::from_slice(&created.stdout).expect("created JSON");
    assert_eq!(created["status"], "created");
    assert_eq!(created["report_digest"].as_str().map(str::len), Some(64));

    let unchanged = ensure();
    assert!(unchanged.status.success());
    let unchanged: Value = serde_json::from_slice(&unchanged.stdout).expect("unchanged JSON");
    assert_eq!(unchanged["status"], "unchanged");
    assert_eq!(unchanged["report_digest"], created["report_digest"]);

    let mut drifted: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).unwrap()).unwrap();
    drifted["generated_at"] = Value::String("2026-01-01T00:00:00Z".to_string());
    fs::write(&report_path, serde_json::to_vec_pretty(&drifted).unwrap()).unwrap();
    let drift = ensure();
    assert_eq!(drift.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&drift.stderr).contains("baseline_drift"));

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "baseline",
            "ensure",
            "--name",
            "audit",
            "--report",
            report_path.to_str().unwrap(),
            "--replace",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\": \"replaced\""));
}

#[test]
fn release_manifest_schema_accepts_the_canonical_fixture_and_rejects_digest_drift() {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/release-manifest-3.json"))
        .expect("release manifest schema");
    let validator = jsonschema::options()
        .build(&schema)
        .expect("compiled release manifest schema");
    let mut manifest: Value = serde_json::from_str(include_str!(
        "../../xtask/tests/fixtures/release-manifest-v0.9.0.json"
    ))
    .expect("release manifest fixture");
    assert!(validator.is_valid(&manifest));
    manifest["artifacts"][0]["sha256"] = json!("abcd");
    let pointers = validator
        .iter_errors(&manifest)
        .map(|error| error.instance_path().as_str().to_string())
        .collect::<Vec<_>>();
    assert!(
        pointers
            .iter()
            .any(|pointer| pointer == "/artifacts/0/sha256"),
        "unexpected schema pointers: {pointers:?}"
    );
}
