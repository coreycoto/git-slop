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
        .args(["find", "--quiet", "--no-cache", "--persist-unadopted"])
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
