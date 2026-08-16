#[test]
fn help_lists_the_current_command_surface_only() {
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .arg("--help")
        .output()
        .expect("run help");
    assert!(
        output.status.success(),
        "help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    for command in [
        "init", "find", "show", "explain", "plan", "check", "compare", "sarif", "health", "version",
    ] {
        assert!(
            stdout.contains(&format!("\n  {command}")),
            "help omitted {command}:\n{stdout}"
        );
    }
    assert!(!stdout.contains("refactor-preview"));
}

#[test]
fn removed_refactor_preview_subcommand_is_rejected_by_clap() {
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .arg("refactor-preview")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unrecognized subcommand"))
        .stderr(predicate::str::contains("refactor-preview"));
}

#[test]
fn completions_are_generated_from_the_live_command_tree() {
    let outside_repository = TempDir::new().expect("temporary non-repository directory");
    for shell in ["bash", "zsh", "fish", "powershell", "nushell"] {
        cargo_bin_cmd!("git-slop")
            .current_dir(outside_repository.path())
            .args(["completions", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("git-slop"))
            .stdout(predicate::str::contains("compare"));
    }
}

#[test]
fn generated_manual_has_no_trailing_whitespace() {
    let outside_repository = TempDir::new().expect("temporary non-repository directory");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(outside_repository.path())
        .arg("man")
        .output()
        .expect("generate manual");
    assert!(
        output.status.success(),
        "manual generation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manual = String::from_utf8(output.stdout).expect("manual is UTF-8");
    assert!(manual.ends_with('\n'));
    assert!(
        manual.lines().all(|line| line.trim_end() == line),
        "generated manual contains trailing whitespace"
    );
}

#[test]
fn generated_reference_has_one_final_newline() {
    let output = cargo_bin_cmd!("git-slop")
        .arg("reference")
        .output()
        .expect("generate reference");
    assert!(output.status.success());
    let reference = String::from_utf8(output.stdout).expect("reference is UTF-8");
    assert!(reference.ends_with('\n'));
    assert!(!reference.ends_with("\n\n"));
}

#[test]
fn explain_accepts_a_unique_relationship_id_prefix() {
    let report = fixture("relationship_focused_report.json");
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            "near_duplicate_neighborhood-35e7",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "near_duplicate_neighborhood-35e7fad1c4e0",
        ));
}

#[test]
fn html_export_is_self_contained_and_searchable() {
    let temporary = TempDir::new().expect("temporary directory");
    let output = temporary.path().join("report.html");
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args(["html", "--report"])
        .arg(fixture("relationship_focused_report.json"))
        .arg("--output")
        .arg(&output)
        .assert()
        .success();
    let html = fs::read_to_string(output).expect("HTML report");
    assert!(html.contains("type=\"application/json\""));
    assert!(html.contains("placeholder=\"Search paths\""));
    assert!(html.contains("near_duplicate_neighborhood"));
    assert!(html.contains("class=\"file-link\""));
    assert!(html.contains("record.member_paths"));
    assert!(html.contains("\"source_report\":null"));
    assert!(!html.contains("https://cdn"));
}

#[test]
fn report_consumers_preserve_missing_report_exit_two() {
    let temporary = TempDir::new().expect("temporary directory");
    let missing = temporary.path().join("missing-report.json");
    let missing_display = missing.to_string_lossy().into_owned();

    for args in [
        vec!["show", "README.md", "--report", &missing_display],
        vec!["check", "--report", &missing_display],
        vec![
            "explain",
            "--path",
            "README.md",
            "--report",
            &missing_display,
        ],
        vec!["plan", "--path", "README.md", "--report", &missing_display],
    ] {
        cargo_bin_cmd!("git-slop")
            .current_dir(manifest_dir())
            .args(&args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(format!(
                "Report not found. Searched: {missing_display}"
            )))
            .stderr(predicate::str::contains("git slop find"));
    }
}

#[test]
fn check_rejects_the_removed_priority_band_flag() {
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args(["check", "--fail-on-priority-band", "critical"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"))
        .stderr(predicate::str::contains("--fail-on-priority-band"));
}

#[test]
fn explain_prompt_pack_carries_the_native_payload_and_safety_boundary() {
    let temporary = TempDir::new().expect("temporary directory");
    let pack = temporary.path().join("explain-pack");
    let report = fixture("local_repo_folder_report.json");

    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--path",
            "src/git_slop",
            "--prompt-pack",
            pack.to_str().expect("prompt-pack path"),
            "--format",
            "json",
        ])
        .assert()
        .success();

    let context = assert_prompt_pack_safety(&pack);
    assert_eq!(context["command"], "explain");
    assert_eq!(context["payload"]["schema_version"], 2);
    assert_eq!(context["payload"]["target"]["path"], "src/git_slop");
}
