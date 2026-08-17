#[test]
fn report_runtime_and_published_schema_reject_the_same_scalar_mutation_matrix() {
    let repository = fixture_repository();
    let output = tempdir().expect("output");
    let report_path = write_report(repository.path(), output.path());
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    let schema_output = cargo_bin_cmd!("git-slop")
        .args(["schema", "report"])
        .output()
        .expect("schema");
    assert!(schema_output.status.success());
    let schema: Value = serde_json::from_slice(&schema_output.stdout).unwrap();
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("compiled report schema");
    let mutations = [
        (
            "report-profile",
            "/analyzer/report_profile",
            json!("invented"),
        ),
        ("profile", "/files/0/profile", json!("invented")),
        ("context-band", "/files/0/context_band", json!("invented")),
        ("slop-band", "/files/0/slop_band", json!("invented")),
        (
            "analysis-status",
            "/files/0/analysis_status",
            json!("invented"),
        ),
        ("slop-score-high", "/files/0/slop_score", json!(100.001)),
        (
            "context-pressure-low",
            "/files/0/context_pressure",
            json!(-0.001),
        ),
        ("head-sha", "/repo/head_sha", json!("not-a-sha")),
        (
            "worktree-digest",
            "/repo/worktree_state_digest",
            json!("abcd"),
        ),
        ("content-sha", "/files/0/content_sha256", json!("abcd")),
        ("fingerprint", "/files/0/content_fingerprint", json!("abcd")),
        ("scope-digest", "/scope/selected_path_digest", json!("abcd")),
    ];
    for (name, pointer, replacement) in mutations {
        let mut mutated = report.clone();
        *mutated.pointer_mut(pointer).expect("mutation pointer") = replacement;
        assert!(
            !validator.is_valid(&mutated),
            "published schema accepted {name} at {pointer}"
        );
        let path = repository.path().join(format!("mutation-{name}.json"));
        fs::write(&path, serde_json::to_vec(&mutated).unwrap()).unwrap();
        let runtime = cargo_bin_cmd!("git-slop")
            .args(["report", "validate", path.to_str().unwrap()])
            .output()
            .expect("runtime validation");
        assert_eq!(runtime.status.code(), Some(2), "runtime accepted {name}");
        let stderr = String::from_utf8_lossy(&runtime.stderr);
        assert!(stderr.contains(pointer), "{name}: {stderr}");
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
    changed["files"][0]["content_sha256"] = Value::String("f".repeat(64));
    changed["compare_index"]["files"][0]["content_sha256"] = Value::String("f".repeat(64));
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
fn doctor_reports_detector_and_policy_cache_writability_separately() {
    let repository = fixture_repository();
    let blocked_policy_home = repository.path().join("policy-cache-is-a-file");
    fs::write(&blocked_policy_home, "not a directory\n").expect("blocked policy cache fixture");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .env("GIT_SLOP_POLICY_HOME", &blocked_policy_home)
        .args(["doctor", "--format", "json"])
        .output()
        .expect("doctor cache probe");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    assert_eq!(payload["advisor"]["default_mode"], "provider_free_json");
    assert_eq!(
        payload["advisor"]["model_required_for_ordinary_use"],
        false
    );
    assert_eq!(payload["advisor"]["public_inference_enabled"], false);
    assert_eq!(
        payload["cache_writability"]["detector_state"]["writable"],
        true
    );
    assert_eq!(
        payload["cache_writability"]["policy_packs"]["writable"],
        false
    );
    assert!(
        payload["diagnostics"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| {
                item["code"] == "policy_cache_not_writable" && item["severity"] == "warning"
            }))
    );
}

#[test]
fn advisor_capacity_schema_is_strict_and_provider_free() {
    let output = cargo_bin_cmd!("git-slop")
        .args(["schema", "advisor-capacity"])
        .output()
        .expect("advisor capacity schema");
    assert!(output.status.success());
    let schema: Value = serde_json::from_slice(&output.stdout).expect("capacity schema JSON");
    let validator = jsonschema::draft202012::options()
        .build(&schema)
        .expect("compiled capacity schema");
    let mut receipt = json!({
        "schema_version": 1,
        "command": "advisor-capacity",
        "eligible": false,
        "provider_contacted": false,
        "report_accessed": false,
        "model": "openai/gpt-oss-safeguard-20b",
        "model_size_bytes": 13793441254_u64,
        "estimated_peak_memory_bytes": 17179869184_u64,
        "required_available_memory_bytes": 25769803776_u64,
        "required_physical_memory_bytes": 25769803776_u64,
        "host": {
            "physical_memory_bytes": 17179869184_u64,
            "available_memory_bytes": 4294967296_u64,
            "swap_used_bytes": 536870912_u64
        },
        "limits": {
            "minimum_model_size_bytes": 13793441254_u64,
            "minimum_estimated_peak_memory_bytes": 17179869184_u64,
            "minimum_physical_memory_bytes": 25769803776_u64,
            "minimum_available_memory_reserve_bytes": 8589934592_u64,
            "maximum_initial_swap_used_bytes": 268435456_u64,
            "maximum_swap_growth_bytes": 268435456_u64
        },
        "release": {
            "recommendation": "defer",
            "public_inference_enabled": false,
            "decision_record": "docs/benchmarks/safeguard-v1-m2-air-decision.md"
        },
        "blockers": [{
            "code": "physical_memory_below_required",
            "message": "host is too small",
            "actual_bytes": 17179869184_u64,
            "comparison": "minimum",
            "limit_bytes": 25769803776_u64
        }]
    });
    assert!(validator.is_valid(&receipt));
    receipt["provider_contacted"] = json!(true);
    assert!(!validator.is_valid(&receipt));
}

#[test]
fn advisor_benchmark_schema_rejects_nested_contract_drift() {
    let schema: serde_json::Value = serde_json::from_slice(
        &fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("schemas/advisor-benchmark-1.json"),
        )
        .expect("advisor benchmark schema"),
    )
    .expect("advisor benchmark schema JSON");
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("compiled advisor benchmark schema");
    let digest = "a".repeat(64);
    let mut receipt = json!({
        "schema_version": 1,
        "status": "incomplete",
        "configuration": {
            "corpus": "org.git-slop.safeguard-v1-gold",
            "mode": "prepare-only",
            "corpus_sha256": digest,
            "thresholds_sha256": "b".repeat(64),
            "harness_revision": "e".repeat(40),
            "binary_sha256": "f".repeat(64),
            "binary_source_revision": "e".repeat(40),
            "binary_version": "0.16.1",
            "binary_target": "aarch64-apple-darwin",
            "binary_build_source": "workspace",
            "binary_inference_feature_enabled": false
        },
        "system": {
            "architecture": null,
            "os_release": null,
            "macos_version": null,
            "hardware_model": null,
            "physical_memory_bytes": null,
            "cpu": null,
            "privacy": "No username, home path, serial number, hardware UUID, repository path, source excerpt, prompt, or rationale is recorded."
        },
        "repositories": {
            "fixture": {
                "revision": "c".repeat(40),
                "as_of": "2026-01-01T00:00:00Z",
                "report_sha256": "d".repeat(64),
                "matches_expected": true
            }
        },
        "recommended_configuration": null,
        "recommendation": "defer",
        "next_step": "Pin the prepared report fingerprint."
    });
    assert!(validator.is_valid(&receipt));
    receipt["configuration"]["unexpected"] = json!(true);
    assert!(!validator.is_valid(&receipt));
    receipt["configuration"]
        .as_object_mut()
        .expect("configuration object")
        .remove("unexpected");
    receipt["repositories"]["fixture"]["unexpected"] = json!(true);
    assert!(!validator.is_valid(&receipt));
}

#[test]
fn doctor_fails_closed_when_the_active_detector_cache_is_not_writable() {
    let repository = fixture_repository();
    let private_root = repository.path().join(".git/git-slop");
    fs::create_dir_all(&private_root).expect("private state parent");
    fs::write(private_root.join("ephemeral"), "not a directory\n")
        .expect("blocked detector cache fixture");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["doctor", "--format", "json"])
        .output()
        .expect("doctor detector cache probe");
    assert_eq!(output.status.code(), Some(2));
    let payload: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    assert_eq!(payload["status"], "error");
    assert_eq!(payload["scan_ready"], false);
    assert_eq!(
        payload["cache_writability"]["detector_state"]["writable"],
        false
    );
}
