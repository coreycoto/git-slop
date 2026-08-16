#[test]
fn plan_prompt_pack_keeps_backlog_handoff_preview_only() {
    let temporary = TempDir::new().expect("temporary directory");
    let pack = temporary.path().join("plan-pack");
    let report = fixture("relationship_focused_report.json");

    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "plan",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            "near_duplicate_neighborhood-35e7fad1c4e0",
            "--prompt-pack",
            pack.to_str().expect("prompt-pack path"),
            "--format",
            "json",
        ])
        .assert()
        .success();

    let context = assert_prompt_pack_safety(&pack);
    assert_eq!(context["command"], "plan");
    assert_eq!(context["payload"]["schema_version"], 2);
    assert_eq!(
        context["payload"]["backlog_handoff"]["mutation_policy"],
        "preview_only"
    );
}

#[test]
fn prompt_pack_rejects_an_existing_file_target() {
    let temporary = TempDir::new().expect("temporary directory");
    let pack = temporary.path().join("not-a-directory");
    fs::write(&pack, "occupied\n").expect("write occupied target");
    let report = fixture("local_repo_folder_report.json");

    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--top",
            "1",
            "--prompt-pack",
            pack.to_str().expect("prompt-pack path"),
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(format!(
            "Prompt pack path is not a directory: {}",
            pack.display()
        )));
}

#[test]
fn prompt_pack_repository_context_is_explicit_bounded_and_repo_relative() {
    let repository = TempDir::new().expect("temporary repository");
    git(repository.path(), &["init", "-b", "main"]);
    let first = repository
        .path()
        .join("src/consumer_toolkit/github/current_repo.py");
    let second = repository
        .path()
        .join("src/consumer_toolkit/github/shared/current_repo.py");
    fs::create_dir_all(first.parent().expect("first parent")).expect("create first parent");
    fs::create_dir_all(second.parent().expect("second parent")).expect("create second parent");
    fs::write(&first, "def current_repo():\n    return 'current'\n").expect("write first source");
    fs::write(&second, "def current_repo():\n    return 'shared'\n").expect("write second source");
    fs::write(
        repository.path().join("AGENTS.md"),
        "# Repository guidance\n",
    )
    .expect("write guidance");
    fs::write(
        repository.path().join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\n",
    )
    .expect("write manifest");
    let pack = repository.path().join("prompt-pack");

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "explain",
            "--report",
            fixture("relationship_focused_report.json")
                .to_str()
                .expect("fixture path"),
            "--relationship",
            "duplicate_neighborhood-b534129a62cb",
            "--prompt-pack",
            pack.to_str().expect("prompt pack path"),
            "--include-repository-context",
            "--excerpt-bytes",
            "256",
        ])
        .assert()
        .success();

    let context = read_json(&pack.join("context.json"));
    let repository_context = &context["repository_context"];
    assert_eq!(repository_context["included"], true);
    assert_eq!(repository_context["reason"], "explicit_opt_in");
    assert_eq!(repository_context["planning_usable"], true);
    assert_eq!(repository_context["execution_ready"], true);
    assert_eq!(
        repository_context["source_excerpts"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        repository_context["guidance"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(repository_context["truncation"]["per_file_byte_limit"], 256);
    assert_eq!(
        repository_context["verification_commands"][0],
        "cargo fmt --all -- --check"
    );
    for excerpt in repository_context["source_excerpts"]
        .as_array()
        .expect("source excerpts")
    {
        let path = excerpt["path"].as_str().expect("relative path");
        assert!(!Path::new(path).is_absolute());
        assert!(excerpt["bytes_returned"].as_u64().unwrap_or_default() <= 256);
    }
}

#[test]
fn prompt_pack_is_not_execution_ready_when_target_source_is_truncated() {
    let repository = TempDir::new().expect("temporary repository");
    git(repository.path(), &["init", "-b", "main"]);
    let first = repository
        .path()
        .join("src/consumer_toolkit/github/current_repo.py");
    let second = repository
        .path()
        .join("src/consumer_toolkit/github/shared/current_repo.py");
    fs::create_dir_all(first.parent().expect("first parent")).expect("create first parent");
    fs::create_dir_all(second.parent().expect("second parent")).expect("create second parent");
    fs::write(&first, "x".repeat(140_000)).expect("write oversized source");
    fs::write(&second, "def current_repo():\n    return 'shared'\n").expect("write source");
    fs::write(repository.path().join("AGENTS.md"), "# Guidance\n").expect("guidance");
    let pack = repository.path().join("prompt-pack");

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "explain",
            "--report",
            fixture("relationship_focused_report.json")
                .to_str()
                .expect("fixture path"),
            "--relationship",
            "duplicate_neighborhood-b534129a62cb",
            "--prompt-pack",
            pack.to_str().expect("prompt pack path"),
            "--include-repository-context",
            "--excerpt-bytes",
            "256",
        ])
        .assert()
        .success();

    let context = read_json(&pack.join("context.json"));
    let repository_context = &context["repository_context"];
    assert_eq!(repository_context["planning_usable"], true);
    assert_eq!(repository_context["execution_ready"], false);
    assert_eq!(repository_context["execution_usable"], false);
    assert_eq!(repository_context["truncation"]["source_complete"], false);
    assert_eq!(repository_context["source_excerpts"][0]["truncated"], true);
}
