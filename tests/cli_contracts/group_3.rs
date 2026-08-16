#[test]
fn prompt_pack_rejects_report_metadata_absolute_and_symlink_escape_paths() {
    let repository = TempDir::new().expect("temporary repository");
    git(repository.path(), &["init", "-b", "main"]);
    let outside = TempDir::new().expect("outside directory");
    let secret = outside.path().join("github/current_repo.py");
    let second = outside.path().join("github/shared/current_repo.py");
    fs::create_dir_all(secret.parent().expect("secret parent")).expect("secret directory");
    fs::create_dir_all(second.parent().expect("second parent")).expect("second directory");
    fs::write(&secret, "must not be copied\n").expect("write outside file");
    fs::write(&second, "must not be copied either\n").expect("write outside file");
    fs::create_dir_all(repository.path().join("src")).expect("source directory");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path(),
        repository.path().join("src/consumer_toolkit"),
    )
    .expect("create escaping symlink");
    fs::write(repository.path().join("AGENTS.md"), "# Guidance\n").expect("guidance");

    let report_path = fixture("relationship_focused_report.json");
    let pack = repository.path().join("prompt-pack");

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "plan",
            "--report",
            report_path.to_str().expect("report path"),
            "--relationship",
            "duplicate_neighborhood-b534129a62cb",
            "--prompt-pack",
            pack.to_str().expect("pack path"),
            "--include-repository-context",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let context = read_json(&pack.join("context.json"));
    let rendered = serde_json::to_string(&context).expect("render context");
    assert!(!rendered.contains("must not be copied"));
    assert!(!rendered.contains(&secret.to_string_lossy().to_string()));
    assert_eq!(context["payload"]["source_report"]["path"], Value::Null);
    assert_eq!(
        context["repository_context"]["truncation"]["source_candidate_count"],
        2
    );
    assert_eq!(context["repository_context"]["execution_ready"], false);
}

#[test]
fn plan_uses_safe_repo_relative_paths_without_exposing_external_local_paths() {
    let report = fixture("relationship_focused_report.json");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "plan",
            "--report",
            report.to_str().expect("report path"),
            "--relationship",
            "near_duplicate_neighborhood-35e7fad1c4e0",
            "--format",
            "json",
        ])
        .output()
        .expect("run plan");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert_eq!(
        payload["source_report"]["path"],
        "tests/fixtures/reports/relationship_focused_report.json"
    );
    assert_eq!(payload["source_report"]["descriptor"], "repo_relative");
    assert!(
        payload["proposed_slices"][0]["baseline_command"]
            .as_str()
            .is_some_and(|command| command
                .contains("tests/fixtures/reports/relationship_focused_report.json"))
    );
}

#[test]
fn init_writes_schema_two_config_ignore_rules_and_state_directories() {
    let repository = TempDir::new().expect("temporary repository");
    git(repository.path(), &["init", "-b", "main"]);

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Initialized .slop/config.yaml (written).",
        ))
        .stdout(predicate::str::contains(
            "Ensured .slop/latest/, .slop/runs/, and .slop/cache/ exist.",
        ));

    let slop = repository.path().join(".slop");
    let config_path = slop.join("config.yaml");
    let gitignore_path = slop.join(".gitignore");
    assert!(config_path.is_file(), "missing config.yaml");
    assert!(gitignore_path.is_file(), "missing .gitignore");
    for directory in ["latest", "runs", "cache"] {
        assert!(slop.join(directory).is_dir(), "missing {directory}/");
    }

    let config: Value =
        serde_yaml::from_str(&fs::read_to_string(config_path).expect("read generated config.yaml"))
            .expect("parse generated config.yaml");
    assert_eq!(config["schema_version"], 2);
    assert_eq!(
        config.as_object().expect("config object").len(),
        1,
        "init should write the minimal forward-compatible config"
    );

    assert_eq!(
        fs::read_to_string(gitignore_path).expect("read generated .gitignore"),
        "/latest/\n/runs/\n/cache/\n/scan.lock\n/scan.lock.owner\n/prompt-packs/\n/diagnostic-bundle.json\n/advice/\n/config.yaml.bak\n/.gitignore.bak\n"
    );
}
