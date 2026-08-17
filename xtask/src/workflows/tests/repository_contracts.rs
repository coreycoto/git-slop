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
#[test]
fn dogfood_regression_acceptance_is_exact_bounded_and_noncritical() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let manifest_path = fixture.path().join("acceptances.json");
    let comparison_path = fixture.path().join("comparison.json");
    let report_path = fixture.path().join("report.json");
    let base_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let head_sha = "dddddddddddddddddddddddddddddddddddddddd";
    let digest = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fs::write(
        &comparison_path,
        serde_json::to_vec(&serde_json::json!({
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
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &report_path,
        serde_json::to_vec(&serde_json::json!({
            "repo": {"head_sha": head_sha},
            "files": [{"path": "src/reviewed.rs", "content_sha256": digest}]
        }))
        .unwrap(),
    )
    .unwrap();

    let mut manifest = serde_json::json!({
        "schema_version": 1,
        "acceptances": [{
            "base_sha": base_sha,
            "rationale": "reviewed fixture",
            "entries": [{
                "path": "src/reviewed.rs",
                "reason": "material_score_increase",
                "severity": "notice",
                "content_sha256": digest,
                "maximum_slop_score": 12.0
            }]
        }]
    });
    let run = |manifest: &serde_json::Value, base: &str| {
        fs::write(&manifest_path, serde_json::to_vec(manifest).unwrap()).unwrap();
        std::process::Command::new("bash")
            .arg(root.join("scripts/verify-dogfood-regressions.sh"))
            .arg(&manifest_path)
            .arg(&comparison_path)
            .arg(&report_path)
            .arg(base)
            .arg(head_sha)
            .output()
            .unwrap()
    };

    assert!(run(&manifest, base_sha).status.success());
    assert!(
        !run(&manifest, "cccccccccccccccccccccccccccccccccccccccc")
            .status
            .success()
    );
    manifest["acceptances"][0]["entries"][0]["maximum_slop_score"] =
        serde_json::json!(11.9);
    assert!(!run(&manifest, base_sha).status.success());
    manifest["acceptances"][0]["entries"][0]["maximum_slop_score"] =
        serde_json::json!(12.0);
    manifest["acceptances"][0]["entries"][0]["severity"] = serde_json::json!("critical");
    assert!(!run(&manifest, base_sha).status.success());

    let workflow = workflow_text("dogfood.yml");
    let verifier = workflow
        .find("scripts/verify-dogfood-regressions.sh")
        .expect("Dogfood acceptance verifier");
    let absolute_policy = workflow
        .find("Evaluate intentional absolute policy")
        .expect("absolute Dogfood policy");
    assert!(verifier < absolute_policy);
}
