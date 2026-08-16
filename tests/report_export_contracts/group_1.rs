#[test]
fn check_folder_gate_and_detail_pagination_are_explicit() {
    let directory = TempDir::new().expect("temporary report directory");
    let report = complete_fixture(&directory, "local_repo_folder_report.json");
    let file_only = command()
        .args(["check", "--report"])
        .arg(&report)
        .args(["--format", "json"])
        .output()
        .expect("run file-only check");
    assert_exit_code(&file_only, 1);
    let file_only: Value = serde_json::from_slice(&file_only.stdout).unwrap();

    let with_folders = command()
        .args(["check", "--report"])
        .arg(&report)
        .args([
            "--format",
            "json",
            "--include-folders",
            "--details",
            "--offset",
            "1",
            "--limit",
            "2",
        ])
        .output()
        .expect("run folder-aware paginated check");
    assert_exit_code(&with_folders, 1);
    let with_folders: Value = serde_json::from_slice(&with_folders.stdout).unwrap();
    assert_eq!(file_only["gate_scope"], "files");
    assert_eq!(with_folders["gate_scope"], "files_and_folders");
    assert!(with_folders["finding_count"].as_u64() > file_only["finding_count"].as_u64());
    assert_eq!(with_folders["collection"]["offset"], 1);
    let expected_returned = with_folders["finding_count"]
        .as_u64()
        .unwrap()
        .saturating_sub(1)
        .min(2);
    assert_eq!(
        with_folders["collection"]["returned"].as_u64(),
        Some(expected_returned)
    );
    assert!(
        with_folders["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|finding| matches!(finding["record_type"].as_str(), Some("file" | "folder")))
    );
}

#[test]
fn check_evaluate_only_preserves_canonical_findings_without_failing() {
    let directory = TempDir::new().expect("temporary report directory");
    let report = complete_fixture(&directory, "local_repo_folder_report.json");
    let output = command()
        .args(["check", "--report"])
        .arg(&report)
        .args(["--format", "json", "--evaluate-only"])
        .output()
        .expect("run evaluate-only check");
    let payload = stdout_json(&output);
    assert_eq!(payload["passed"], false);
    assert!(
        payload["finding_count"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
}

#[test]
fn compare_json_preserves_status_deltas_and_queue_movement() {
    let directory = TempDir::new().expect("temporary report directory");
    let base = complete_fixture(&directory, "compare_base_report.json");
    let head = complete_fixture(&directory, "compare_head_report.json");
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
    assert_eq!(payload["report_schema_version"], 5);
    assert_eq!(payload["command"], "compare");
    assert_eq!(payload["base_report"]["repo_name"], "compare-fixture");
    assert_eq!(
        payload["base_report"]["head_sha"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        payload["head_report"]["head_sha"],
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(payload["summary"]["files"]["added"], 1);
    assert_eq!(payload["summary"]["files"]["removed"], 1);
    assert_eq!(payload["summary"]["files"]["changed"], 2);
    assert_eq!(payload["summary"]["files"]["unchanged"], 0);
    assert_eq!(payload["summary"]["folders"]["changed"], 1);
    assert_eq!(payload["summary"]["worsened_file_count"], 1);
    assert_eq!(payload["summary"]["improved_file_count"], 1);

    let a = item_by_path(&payload["file_deltas"], "src/a.py");
    assert_eq!(a["status"], "source_changed");
    assert_eq!(a["slop_score_delta"], 50.0);
    assert_eq!(a["token_delta"], 60);
    assert_eq!(a["load_pressure_delta"], 0.5);
    assert_eq!(a["context_band_delta"], 2);
    assert_eq!(a["slop_band_delta"], 2);
    assert_eq!(a["overlay_deltas"][0]["label"], "verification");
    assert_eq!(a["overlay_deltas"][0]["delta"], 0.6);

    let b = item_by_path(&payload["file_deltas"], "src/b.py");
    assert_eq!(b["status"], "source_changed");
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
    files[1]["overlays"] = json!({});
    files[2]["overlays"] = json!({});
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
