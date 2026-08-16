#[test]
fn compare_text_surface_honors_top_and_explains_its_boundary() {
    let directory = TempDir::new().expect("temporary report directory");
    let base = complete_fixture(&directory, "compare_base_report.json");
    let head = complete_fixture(&directory, "compare_head_report.json");
    let output = command()
        .args(["compare", "--base"])
        .arg(base)
        .arg("--head")
        .arg(head)
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
    let valid_base = complete_fixture(&directory, "compare_base_report.json");
    let valid_head = complete_fixture(&directory, "compare_head_report.json");

    let missing_output = command()
        .args(["compare", "--base"])
        .arg(&valid_base)
        .arg("--head")
        .arg(&missing)
        .output()
        .expect("run compare with missing report");
    assert_exit_code(&missing_output, 2);
    assert!(
        String::from_utf8_lossy(&missing_output.stderr).contains(&format!(
            "Report not found. Searched: {}",
            missing.display()
        ))
    );

    let invalid_output = command()
        .args(["compare", "--base"])
        .arg(invalid)
        .arg("--head")
        .arg(&valid_head)
        .output()
        .expect("run compare with invalid report");
    assert_exit_code(&invalid_output, 2);
    assert!(String::from_utf8_lossy(&invalid_output.stderr).contains("schema_version must be 5"));

    let mut default_head_command = command();
    default_head_command.current_dir(directory.path());
    let incomplete_output = default_head_command
        .args(["compare", "--base"])
        .arg(&valid_base)
        .output()
        .expect("run compare with missing default head");
    assert_exit_code(&incomplete_output, 2);
    let incomplete_stderr = String::from_utf8_lossy(&incomplete_output.stderr);
    assert!(incomplete_stderr.contains("Report not found. Searched: .slop/latest/report.json"));

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
fn compare_rejects_truncated_compact_and_degraded_inputs_even_with_force() {
    let directory = TempDir::new().expect("temporary report directory");
    let valid = load_fixture("compare_head_report.json");

    let mut compact = load_fixture("compare_base_report.json");
    compact["analyzer"] = json!({"report_profile": "compact"});
    compact["collection_metadata"] = json!({
        "files": {"total": 10, "returned": 3, "limit": 3, "truncated": true},
        "folders": {"total": 1, "returned": 1, "limit": null, "truncated": false}
    });
    let compact_path = write_report(&directory, "compact.json", &compact);
    let valid_path = write_report(&directory, "valid.json", &valid);
    let compact_output = command()
        .args(["compare", "--base"])
        .arg(&compact_path)
        .arg("--head")
        .arg(&valid_path)
        .args(["--force", "--fail-on-regression"])
        .output()
        .expect("run compact compare");
    assert_exit_code(&compact_output, 2);
    let compact_stderr = String::from_utf8_lossy(&compact_output.stderr);
    assert!(
        compact_stderr.contains("exhaustive policy index"),
        "{compact_stderr}"
    );

    let mut degraded = load_fixture("compare_base_report.json");
    degraded["diagnostics"] = json!({
        "analysis": {"analysis_status": "degraded_resource_budget"}
    });
    let degraded_path = write_report(&directory, "degraded.json", &degraded);
    let degraded_output = command()
        .args(["compare", "--base"])
        .arg(&degraded_path)
        .arg("--head")
        .arg(&valid_path)
        .arg("--force")
        .output()
        .expect("run degraded compare");
    assert_exit_code(&degraded_output, 2);
    assert!(
        String::from_utf8_lossy(&degraded_output.stderr)
            .contains("analysis status is degraded_resource_budget")
    );

    let mut shallow = load_fixture("compare_base_report.json");
    shallow["evidence_completeness"] = json!({"history": "incomplete_shallow"});
    let shallow_path = write_report(&directory, "shallow.json", &shallow);
    let shallow_output = command()
        .args(["compare", "--base"])
        .arg(shallow_path)
        .arg("--head")
        .arg(valid_path)
        .arg("--force")
        .output()
        .expect("run shallow compare");
    assert_exit_code(&shallow_output, 2);
    assert!(
        String::from_utf8_lossy(&shallow_output.stderr)
            .contains("evidence status is incomplete_shallow")
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
    base["scope"] = json!({
        "mode":"scoped",
        "path":"src",
        "selected_path_count":3,
        "selected_path_digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    });
    head["scope"] = json!({
        "mode":"scoped",
        "path":"lib",
        "selected_path_count":4,
        "selected_path_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    });
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
            .is_some_and(|items| items.len() == 1 && items[0]["pointer"] == "/scope/path")
    );
}
