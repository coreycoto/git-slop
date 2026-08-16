#[test]
fn find_rejects_escaping_missing_and_empty_scopes() {
    let repository = committed_repository();
    for scope in ["../outside", "/absolute/path", "missing"] {
        cargo_bin_cmd!("git-slop")
            .current_dir(repository.path())
            .args(["find", "--quiet", "--scope", scope])
            .assert()
            .failure();
    }

    fs::create_dir_all(repository.path().join("empty")).expect("empty directory");
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--scope", "empty"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("selected no tracked paths"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--scope", "empty", "--allow-empty-scope"])
        .assert()
        .success();
}

#[test]
fn report_validate_rejects_a_missing_canonical_nested_field() {
    let repository = committed_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--persist-unadopted"])
        .assert()
        .success();
    let report_path = repository.path().join(".slop/latest/report.json");
    cargo_bin_cmd!("git-slop")
        .args([
            "report",
            "validate",
            report_path.to_str().expect("report path"),
        ])
        .assert()
        .success();

    let mut report: Value =
        serde_json::from_slice(&fs::read(&report_path).expect("report")).expect("report JSON");
    report["files"][0]
        .as_object_mut()
        .expect("file")
        .remove("content_fingerprint");
    fs::write(
        &report_path,
        serde_json::to_vec(&report).expect("serialize"),
    )
    .expect("write invalid report");
    cargo_bin_cmd!("git-slop")
        .args([
            "report",
            "validate",
            report_path.to_str().expect("report path"),
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("content_fingerprint"));
}

#[test]
fn explicit_relative_report_paths_are_resolved_against_global_repo() {
    let repository = committed_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--persist-unadopted"])
        .assert()
        .success();
    let outside_repository = TempDir::new().expect("temporary non-repository directory");
    cargo_bin_cmd!("git-slop")
        .current_dir(outside_repository.path())
        .args([
            "--repo",
            repository.path().to_str().expect("repository path"),
            "report",
            "validate",
            ".slop/latest/report.json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Report is valid"));
    cargo_bin_cmd!("git-slop")
        .current_dir(outside_repository.path())
        .args([
            "--repo",
            repository.path().to_str().expect("repository path"),
            "health",
            "--report",
            ".slop/latest/report.json",
            "--format",
            "json",
        ])
        .assert()
        .success();
}

#[test]
fn relationship_plan_matches_json_and_text_goldens() {
    let report = fixture("relationship_focused_report.json");
    let relationship = "near_duplicate_neighborhood-35e7fad1c4e0";

    let json_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "plan",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--format",
            "json",
        ])
        .output()
        .expect("run plan");
    assert!(json_output.status.success());
    let actual: Value = serde_json::from_slice(&json_output.stdout).expect("plan JSON");
    let expected: Value = serde_json::from_slice(
        &fs::read(fixture("relationship_focused_plan.json")).expect("golden JSON"),
    )
    .expect("parse golden JSON");
    assert_eq!(actual, expected);

    let text_golden = fixture("relationship_focused_plan.txt");
    let text_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "plan",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--format",
            "text",
        ])
        .output()
        .expect("run text plan");
    assert_stdout_matches_golden(&text_output, &text_golden);
}

#[test]
fn show_preserves_same_id_memberships_across_cluster_kinds() {
    let report = fixture("relationship_focused_report.json");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "show",
            "src/consumer_toolkit/github/current_repo.py",
            "--report",
            report.to_str().expect("fixture path"),
            "--format",
            "json",
        ])
        .output()
        .expect("run show");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("show JSON");
    let memberships = payload["cluster_memberships"]
        .as_array()
        .expect("cluster memberships");
    let matching_kinds: Vec<&str> = memberships
        .iter()
        .filter(|membership| membership["id"] == "duplicate_set-ce293b441009")
        .filter_map(|membership| membership["kind"].as_str())
        .collect();
    assert_eq!(matching_kinds, ["duplicate_set", "consolidation_candidate"]);
}

#[test]
fn relationship_explain_matches_rich_text_golden_without_changing_json_contract() {
    let report = fixture("relationship_focused_report.json");
    let relationship = "near_duplicate_neighborhood-35e7fad1c4e0";
    let text_golden = fixture("relationship_focused_explain.txt");

    let text_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--verbose",
            "--format",
            "text",
        ])
        .output()
        .expect("run text relationship explain");
    assert_stdout_matches_golden(&text_output, &text_golden);

    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--format",
            "json",
        ])
        .output()
        .expect("run relationship explain");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("relationship explain JSON");
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(
        payload["target"]["relationship_kind"],
        "near_duplicate_neighborhood"
    );
    assert_eq!(
        payload["cost_summary"]["source"]["costs"]["load"]["file_token_count"],
        490
    );
    assert_eq!(
        payload["overlay_summary"]["target_overlays"]["concept_dispersion"]["concept_dispersion_pressure"],
        1.0
    );
}
