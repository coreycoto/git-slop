#[test]
fn every_external_action_surface_requires_a_full_commit_sha() {
    let root = tempfile::tempdir().unwrap();
    let workflows = root.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(
        root.path().join("action.yml"),
        "runs:\n  using: composite\n  steps:\n    - uses: actions/cache@0123456789abcdef0123456789abcdef01234567\n",
    )
    .unwrap();
    fs::write(
        workflows.join("unsafe.yml"),
        "jobs:\n  unsafe:\n    uses: owner/reusable@v1\n",
    )
    .unwrap();
    let mut errors = Vec::new();
    validate_action_versions(root.path(), &workflows, &mut errors);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("owner/reusable@v1"));
}

#[test]
fn packaged_contract_validation_requires_a_clean_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let valid = fs::read_to_string(root.join("scripts/validate-packaged-contracts.sh")).unwrap();
    let invalid = valid.replacen(
        "git clone --quiet --no-hardlinks --no-tags \"$source_worktree\" \"$worktree\"",
        "cp -R \"$source_worktree\" \"$worktree\"",
        1,
    );
    let mut errors = Vec::new();
    validate_packaged_contracts_text(&invalid, &mut errors);
    assert!(errors.iter().any(|error| error.contains("git clone")));
}

#[cfg(unix)]
fn dogfood_acceptance(base_sha: &str, digest: &str) -> serde_json::Value {
    serde_json::json!({
        "base_sha": base_sha,
        "rationale": "reviewed fixture",
        "entries": [{
            "path": "src/reviewed.rs",
            "reason": "material_score_increase",
            "severity": "notice",
            "content_sha256": digest,
            "maximum_slop_score": 12.0
        }]
    })
}

#[cfg(unix)]
fn dogfood_fixture_documents(
    base_sha: &str,
    head_sha: &str,
    digest: &str,
) -> (serde_json::Value, serde_json::Value) {
    let comparison = serde_json::json!({
        "command": "compare",
        "schema_version": 1,
        "detail": "full",
        "policy_source": "base",
        "base_report": {"head_sha": base_sha},
        "head_report": {"head_sha": head_sha},
        "pagination": {"regressions": {"has_more": false}},
        "summary": {"regression_count": 1},
        "regressions": [{
            "path": "src/reviewed.rs",
            "reason": "material_score_increase",
            "severity": "notice",
            "head_slop_score": 12.0
        }]
    });
    let report = serde_json::json!({
        "repo": {"head_sha": head_sha},
        "files": [{"path": "src/reviewed.rs", "content_sha256": digest}]
    });
    (comparison, report)
}

#[cfg(unix)]
fn run_dogfood_verifier(
    repo_root: &Path,
    manifest_path: &Path,
    comparison_path: &Path,
    report_path: &Path,
    base_sha: &str,
    head_sha: &str,
) -> std::process::Output {
    std::process::Command::new("bash")
        .arg(repo_root.join("scripts/verify-dogfood-regressions.sh"))
        .arg(manifest_path)
        .arg(comparison_path)
        .arg(report_path)
        .arg(base_sha)
        .arg(head_sha)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn write_dogfood_manifest(path: &Path, acceptances: Vec<serde_json::Value>) {
    fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "acceptances": acceptances
        }))
        .unwrap(),
    )
    .unwrap();
}

#[cfg(unix)]
fn dogfood_verifier_fixture(
    fixture: &tempfile::TempDir,
    base_sha: &str,
    head_sha: &str,
    digest: &str,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let manifest_path = fixture.path().join("acceptances.json");
    let comparison_path = fixture.path().join("comparison.json");
    let report_path = fixture.path().join("report.json");
    let (comparison, report) = dogfood_fixture_documents(base_sha, head_sha, digest);
    fs::write(&comparison_path, serde_json::to_vec(&comparison).unwrap()).unwrap();
    fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    (manifest_path, comparison_path, report_path)
}

#[cfg(unix)]
#[test]
fn dogfood_verifier_supports_single_manifest_and_adjacent_shards() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let base_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let head_sha = "dddddddddddddddddddddddddddddddddddddddd";
    let digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let (manifest_path, comparison_path, report_path) =
        dogfood_verifier_fixture(&fixture, base_sha, head_sha, digest);
    let mut root_acceptance = dogfood_acceptance(base_sha, digest);
    write_dogfood_manifest(&manifest_path, vec![root_acceptance.clone()]);
    let run = |base: &str| {
        run_dogfood_verifier(
            root,
            &manifest_path,
            &comparison_path,
            &report_path,
            base,
            head_sha,
        )
    };
    assert!(run(base_sha).status.success());
    assert!(
        !run("cccccccccccccccccccccccccccccccccccccccc")
            .status
            .success()
    );

    let extensionless_manifest_path = fixture.path().join("legacy-acceptances");
    write_dogfood_manifest(&extensionless_manifest_path, vec![root_acceptance.clone()]);
    assert!(
        run_dogfood_verifier(
            root,
            &extensionless_manifest_path,
            &comparison_path,
            &report_path,
            base_sha,
            head_sha,
        )
        .status
        .success()
    );

    root_acceptance["entries"][0]["maximum_slop_score"] = serde_json::json!(11.9);
    write_dogfood_manifest(&manifest_path, vec![root_acceptance.clone()]);
    assert!(!run(base_sha).status.success());

    root_acceptance["entries"][0]["maximum_slop_score"] = serde_json::json!(12.0);
    let unrelated_base = "cccccccccccccccccccccccccccccccccccccccc";
    let unrelated = dogfood_acceptance(unrelated_base, digest);
    write_dogfood_manifest(&manifest_path, vec![unrelated.clone()]);
    let shard_dir = fixture.path().join("acceptances");
    fs::create_dir_all(&shard_dir).unwrap();
    let shard_path = shard_dir.join(format!("{base_sha}.json"));
    write_dogfood_manifest(&shard_path, vec![root_acceptance.clone()]);
    assert!(run(base_sha).status.success());

    write_dogfood_manifest(&manifest_path, vec![root_acceptance.clone()]);
    assert!(
        !run(base_sha).status.success(),
        "duplicate base across inputs"
    );

    write_dogfood_manifest(&manifest_path, vec![unrelated.clone()]);
    let mut invalid_schema = serde_json::json!({
        "schema_version": 2,
        "acceptances": [root_acceptance.clone()]
    });
    fs::write(&shard_path, serde_json::to_vec(&invalid_schema).unwrap()).unwrap();
    assert!(!run(base_sha).status.success(), "invalid shard schema");

    invalid_schema["schema_version"] = serde_json::json!(1);
    for severity in ["error", "critical"] {
        invalid_schema["acceptances"][0]["entries"][0]["severity"] = serde_json::json!(severity);
        fs::write(&shard_path, serde_json::to_vec(&invalid_schema).unwrap()).unwrap();
        assert!(
            !run(base_sha).status.success(),
            "{severity} severity is not acceptable"
        );
    }

    write_dogfood_manifest(&manifest_path, vec![root_acceptance.clone()]);
    fs::remove_dir_all(&shard_dir).unwrap();
    let mut drifted_report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    drifted_report["files"][0]["content_sha256"] = serde_json::json!("e".repeat(64));
    fs::write(&report_path, serde_json::to_vec(&drifted_report).unwrap()).unwrap();
    assert!(!run(base_sha).status.success(), "changed content digest");

    drifted_report["files"][0]["content_sha256"] = serde_json::json!(digest);
    fs::write(&report_path, serde_json::to_vec(&drifted_report).unwrap()).unwrap();
    assert!(
        run(base_sha).status.success(),
        "restored accepted content digest"
    );

    let mut comparison: serde_json::Value =
        serde_json::from_slice(&fs::read(&comparison_path).unwrap()).unwrap();
    comparison["regressions"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "path": "src/extra.rs",
            "reason": "new_finding",
            "severity": "notice",
            "head_slop_score": 1.0
        }));
    comparison["summary"]["regression_count"] = serde_json::json!(2);
    let mut report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    report["files"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "path": "src/extra.rs",
            "content_sha256": digest
        }));
    fs::write(&comparison_path, serde_json::to_vec(&comparison).unwrap()).unwrap();
    fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    assert!(!run(base_sha).status.success(), "unaccepted extra finding");

    let workflow = workflow_text("dogfood.yml");
    let verifier = workflow
        .find("scripts/verify-dogfood-regressions.sh")
        .expect("Dogfood acceptance verifier");
    let absolute_policy = workflow
        .find("Evaluate intentional absolute policy")
        .expect("absolute Dogfood policy");
    assert!(verifier < absolute_policy);
}

#[cfg(unix)]
#[test]
fn dogfood_native_author_validation_inspects_root_and_shard_manifests() {
    let fixture = tempfile::tempdir().unwrap();
    let config = fixture.path().join("config/github");
    let root_path = config.join("dogfood-regression-acceptances.json");
    let shard_dir = config.join("dogfood-regression-acceptances");
    let base = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let other_base = "cccccccccccccccccccccccccccccccccccccccc";
    let digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    fs::create_dir_all(&shard_dir).unwrap();
    write_dogfood_manifest(&root_path, vec![dogfood_acceptance(other_base, digest)]);
    let shard_path = shard_dir.join(format!("{base}.json"));
    write_dogfood_manifest(&shard_path, vec![dogfood_acceptance(base, digest)]);

    let validate = || {
        let mut errors = Vec::new();
        dogfood::validate_acceptance_manifests(fixture.path(), &mut errors);
        errors
    };
    assert!(validate().is_empty());

    write_dogfood_manifest(&root_path, vec![dogfood_acceptance(base, digest)]);
    let duplicate_errors = validate();
    assert!(
        duplicate_errors
            .iter()
            .any(|error| error.contains("schema"))
    );

    write_dogfood_manifest(&root_path, vec![dogfood_acceptance(other_base, digest)]);
    let invalid_schema = serde_json::json!({
        "schema_version": 2,
        "acceptances": [dogfood_acceptance(base, digest)]
    });
    fs::write(&shard_path, serde_json::to_vec(&invalid_schema).unwrap()).unwrap();
    assert!(!validate().is_empty());

    for severity in ["error", "critical"] {
        let mut invalid_entry = dogfood_acceptance(base, digest);
        invalid_entry["entries"][0]["severity"] = serde_json::json!(severity);
        write_dogfood_manifest(&shard_path, vec![invalid_entry]);
        assert!(!validate().is_empty(), "{severity} severity is forbidden");
    }
}

#[test]
fn dogfood_regression_failure_retains_full_reports_only_for_that_failure() {
    let workflow = workflow_text("dogfood.yml");
    let enforcement = workflow
        .split_once("      - name: Enforce pull-request regressions")
        .and_then(|(_, tail)| {
            tail.split_once("      - name: Upload full Dogfood regression evidence on failure")
        })
        .map(|(block, _)| block)
        .expect("Dogfood enforcement step precedes its failure artifact");
    assert!(enforcement.contains("id: regressions"));

    let evidence = workflow
        .split_once("      - name: Upload full Dogfood regression evidence on failure")
        .and_then(|(_, tail)| tail.split_once("      - name: Preview first-adoption comparison"))
        .map(|(block, _)| block)
        .expect("Dogfood failure evidence step precedes first-adoption preview");
    for required in [
        "if: failure() && steps.regressions.outcome == 'failure'",
        "uses: actions/upload-artifact@",
        "${{ runner.temp }}/dogfood-comparison.json",
        ".slop/latest/report.json",
        "if-no-files-found: warn",
        "retention-days: 14",
    ] {
        assert!(evidence.contains(required), "missing {required}");
    }
    assert!(
        evidence
            .contains("git-slop-dogfood-failure-${{ github.run_id }}-${{ github.run_attempt }}")
    );
}

#[cfg(unix)]
#[test]
fn dogfood_enforcement_emits_context_before_rejection_and_preserves_evidence() {
    use std::os::unix::fs::PermissionsExt;

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = parsed(&workflow_text("dogfood.yml"));
    let enforcement = workflow["jobs"]["dogfood"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| step["id"].as_str() == Some("regressions"))
        .unwrap()["run"]
        .as_str()
        .unwrap();
    let base = "a".repeat(40);
    let head = "b".repeat(40);
    let digest = "c".repeat(64);
    for rejected in [false, true] {
        let fixture = tempfile::tempdir().unwrap();
        for directory in ["target/release", "scripts", "config/github", ".slop/latest", "runner"] {
            fs::create_dir_all(fixture.path().join(directory)).unwrap();
        }
        let binary = fixture.path().join("target/release/git-slop");
        fs::write(&binary, "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \" $* \" == *\" --format json \"* ]]; then\n  cat \"$COMPARISON_FIXTURE\"\nelse\n  printf '%s\\n' 'bounded native comparison rendered'\nfi\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let verifier = fixture.path().join("scripts/verify-dogfood-regressions.sh");
        fs::copy(repo_root.join("scripts/verify-dogfood-regressions.sh"), &verifier).unwrap();
        fs::set_permissions(&verifier, fs::Permissions::from_mode(0o755)).unwrap();
        let manifest = fixture.path().join("config/github/dogfood-regression-acceptances.json");
        write_dogfood_manifest(&manifest, vec![dogfood_acceptance(&base, &digest)]);
        let manifest_bytes = fs::read(&manifest).unwrap();
        let (mut comparison, mut report) = dogfood_fixture_documents(&base, &head, &digest);
        let hostile_path = "src/unreviewed\n::error::[click](https://example.test).rs";
        if rejected {
            comparison["regressions"][0]["path"] = serde_json::json!(hostile_path);
            report["files"][0]["path"] = serde_json::json!(hostile_path);
        }
        let comparison_bytes = serde_json::to_vec(&comparison).unwrap();
        let report_bytes = serde_json::to_vec(&report).unwrap();
        let comparison_fixture = fixture.path().join("comparison-fixture.json");
        fs::write(&comparison_fixture, &comparison_bytes).unwrap();
        let head_report = fixture.path().join(".slop/latest/report.json");
        fs::write(&head_report, &report_bytes).unwrap();
        let summary = fixture.path().join("summary.md");
        let runner = fixture.path().join("runner");
        let output = std::process::Command::new("bash")
            .args(["-c", enforcement])
            .current_dir(fixture.path())
            .env("COMPARISON_FIXTURE", &comparison_fixture)
            .env("RUNNER_TEMP", &runner)
            .env("BASE_SHA", &base)
            .env("HEAD_SHA", &head)
            .env("GITHUB_STEP_SUMMARY", &summary)
            .env("GITHUB_RUN_ID", "123")
            .env("GITHUB_RUN_ATTEMPT", "2")
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(if rejected { 1 } else { 0 }), "{stderr}");
        assert!(stdout.contains(&format!("Dogfood comparison: base={base} head={head}")));
        assert!(stdout.contains("bounded native comparison rendered"));
        if rejected {
            assert!(stderr.contains("exceed or drift from the reviewed acceptance ledger"));
            let summary = fs::read_to_string(&summary).unwrap();
            assert!(summary.contains(&base) && summary.contains(&head));
            assert!(summary.contains("git-slop-dogfood-failure-123-2"));
            assert!(summary.contains("content hashes"));
            assert!(!summary.contains("unreviewed") && !summary.contains("::error::"));
            assert!(!stdout.contains(hostile_path) && !stderr.contains(hostile_path));
        } else {
            assert!(stdout.contains("bound 1 reviewed regression(s)"));
            assert!(!summary.exists(), "accepted input must not emit a failure summary");
        }
        assert_eq!(fs::read(runner.join("dogfood-comparison.json")).unwrap(), comparison_bytes);
        assert_eq!(fs::read(head_report).unwrap(), report_bytes);
        assert_eq!(fs::read(manifest).unwrap(), manifest_bytes, "verification mutated acceptance policy");
    }
}

#[test]
fn dogfood_retries_share_the_exact_source_clock_without_loosening_limits() {
    let good = workflow_text("dogfood.yml");
    let mut errors = Vec::new();
    validate_dogfood_analysis_clock(&good, "dogfood.yml", &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    for (from, to) in [
        ("git show -s --format=%cI HEAD", "date -u +%FT%TZ"),
        ("--as-of \"$ANALYSIS_AS_OF\"", ""),
        (
            "ANALYSIS_AS_OF: ${{ steps.analysis-clock.outputs.as_of }}",
            "ANALYSIS_AS_OF: ${{ github.event.pull_request.base.sha }}",
        ),
    ] {
        let changed = good.replacen(from, to, 1);
        assert_ne!(changed, good, "mutation must alter the real workflow");
        let mut errors = Vec::new();
        validate_dogfood_analysis_clock(&changed, "dogfood.yml", &mut errors);
        assert!(!errors.is_empty(), "accepted unstable comparison: {from}");
    }
}
