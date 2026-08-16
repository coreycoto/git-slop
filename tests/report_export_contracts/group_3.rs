#[test]
fn sarif_stdout_preserves_sarif_tool_finding_and_evidence_contracts() {
    let report_path = fixture("large_repo_top_report.json");
    let report = load_fixture("large_repo_top_report.json");
    let output = command()
        .args(["sarif", "--report"])
        .arg(&report_path)
        .args(["--top", "2"])
        .output()
        .expect("run SARIF command");
    let payload = stdout_json(&output);

    assert_eq!(
        payload["$schema"],
        "https://json.schemastore.org/sarif-2.1.0.json"
    );
    assert_eq!(payload["version"], "2.1.0");
    let run = &payload["runs"][0];
    let driver = &run["tool"]["driver"];
    assert_eq!(driver["name"], "git-slop");
    assert_eq!(
        driver["informationUri"],
        "https://github.com/coreycoto/git-slop"
    );
    assert_eq!(driver["rules"].as_array().map(Vec::len), Some(2));
    let rule = &driver["rules"][0];
    assert_eq!(rule["id"], "git-slop.context-budget");
    assert_eq!(rule["name"], "Git Slop context budget");
    assert_eq!(rule["properties"]["precision"], "medium");
    assert!(
        rule["properties"]["tags"]
            .as_array()
            .expect("rule tags")
            .contains(&json!("maintainability"))
    );
    assert_eq!(run["automationDetails"]["id"], "git-slop/sarif");
    assert_eq!(
        run["versionControlProvenance"][0]["repositoryUri"],
        "large-validation-repo"
    );

    let results = run["results"].as_array().expect("SARIF results");
    assert_eq!(results.len(), 2);
    for (index, result) in results.iter().enumerate() {
        let expected_path = &report["action_queue"][index]["path"];
        assert!(matches!(
            result["ruleId"].as_str(),
            Some("git-slop.context-budget" | "git-slop.maintenance-pressure")
        ));
        assert_eq!(result["level"], "warning");
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            *expected_path
        );
        assert_eq!(result["properties"]["git_slop"]["rank"], index + 1);
        assert!(result["properties"]["git_slop"]["classification"].is_string());
        assert!(result["properties"]["git_slop"]["remediation_kind"].is_string());
        assert!(result["properties"]["git_slop"]["costs"].is_object());
        assert!(result["properties"]["git_slop"]["strongest_overlays"].is_object());
    }
    let first_evidence = &results[0]["properties"]["git_slop"];
    assert_eq!(first_evidence["costs"], report["files"][0]["costs"]);
    assert_eq!(
        first_evidence["strongest_overlays"]["concept_dispersion"],
        1.0
    );
    assert_eq!(
        first_evidence["strongest_overlays"]["blast_radius"],
        0.651036
    );
    assert!(
        first_evidence["evidence_boundary"]
            .as_str()
            .expect("evidence boundary")
            .contains("does not rescore")
    );
    assert_eq!(
        run["invocations"][0]["properties"]["git_slop"]["schema_version"],
        1
    );
    assert_eq!(
        run["invocations"][0]["properties"]["git_slop"]["report_schema_version"],
        5
    );
    assert_eq!(
        run["invocations"][0]["properties"]["git_slop"]["report_path"],
        Value::Null
    );
    assert_eq!(
        run["invocations"][0]["properties"]["git_slop"]["boundary_note"],
        SARIF_BOUNDARY
    );
    assert_eq!(
        run["properties"]["git_slop"]["boundary_note"],
        SARIF_BOUNDARY
    );
}

#[test]
fn sarif_writes_the_same_contract_to_a_requested_output_file() {
    let directory = TempDir::new().expect("temporary SARIF directory");
    let output_path = directory.path().join("nested/git-slop.sarif");
    let output = command()
        .args(["sarif", "--report"])
        .arg(fixture("large_repo_top_report.json"))
        .args(["--top", "1", "--output"])
        .arg(&output_path)
        .output()
        .expect("run SARIF file export");
    assert_success(&output);
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains(&format!("Wrote SARIF report to {}.", output_path.display()))
    );
    let payload: Value =
        serde_json::from_slice(&fs::read(&output_path).expect("read SARIF output"))
            .expect("parse SARIF output");
    assert_eq!(payload["version"], "2.1.0");
    assert_eq!(
        payload["runs"][0]["results"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        payload["runs"][0]["results"][0]["properties"]["git_slop"]["rank"],
        1
    );
}

#[test]
fn sarif_reports_missing_invalid_and_zero_top_inputs_as_usage_errors() {
    let directory = TempDir::new().expect("temporary report directory");
    let missing = directory.path().join("missing.json");
    let mut invalid_report = load_fixture("large_repo_top_report.json");
    invalid_report["schema_version"] = json!(2);
    let invalid = write_report(&directory, "invalid-schema.json", &invalid_report);
    let valid = fixture("large_repo_top_report.json");

    let missing_output = command()
        .args(["sarif", "--report"])
        .arg(&missing)
        .output()
        .expect("run SARIF with missing report");
    assert_exit_code(&missing_output, 2);
    assert!(
        String::from_utf8_lossy(&missing_output.stderr).contains(&format!(
            "Report not found. Searched: {}",
            missing.display()
        ))
    );

    let invalid_output = command()
        .args(["sarif", "--report"])
        .arg(invalid)
        .output()
        .expect("run SARIF with invalid report");
    assert_exit_code(&invalid_output, 2);
    assert!(String::from_utf8_lossy(&invalid_output.stderr).contains("schema_version must be 5"));

    let bad_top_output = command()
        .args(["sarif", "--report"])
        .arg(valid)
        .args(["--top", "0"])
        .output()
        .expect("run SARIF with zero top");
    assert_exit_code(&bad_top_output, 2);
    assert!(
        String::from_utf8_lossy(&bad_top_output.stderr)
            .contains("--top must be greater than zero.")
    );
}
