#[test]
fn cluster_explain_uses_cluster_kind_and_matches_rich_text_golden() {
    let report = fixture("relationship_focused_report.json");
    let cluster = "duplicate_set-ce293b441009";
    let text_golden = fixture("cluster_focused_explain.txt");

    let text_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--cluster",
            cluster,
            "--verbose",
            "--format",
            "text",
        ])
        .output()
        .expect("run text cluster explain");
    assert_stdout_matches_golden(&text_output, &text_golden);

    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--cluster",
            cluster,
            "--format",
            "json",
        ])
        .output()
        .expect("run cluster explain");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("cluster explain JSON");
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(payload["target"]["cluster_kind"], "duplicate_set");
    assert_eq!(
        payload["target"]["candidate_type"],
        "consolidate_duplicate_knowledge"
    );
    assert_eq!(payload["target"]["top_level_roots"], json!(["src"]));
    assert_eq!(
        payload["cost_summary"]["member_hotspots"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn health_github_is_advisory_capped_actionable_and_escaped() {
    let report = write_report(&health_report());
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.path().to_str().expect("report path"),
            "--format",
            "github",
            "--max-annotations",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "::error file=src/a%2Cb%25file.rs,title=Context budget exceeded::",
        ))
        .stdout(predicate::str::contains(
            "Next: git slop explain --path 'src/a,b%25file.rs'",
        ))
        .stdout(predicate::str::contains("src/second.rs").not());
}

#[test]
fn health_markdown_matches_folder_guidance_golden() {
    let report = fixture("health_folder_guidance_report.json");
    let golden = fixture("health_folder_guidance.md");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.to_str().expect("fixture path"),
            "--format",
            "markdown",
        ])
        .output()
        .expect("run health Markdown");

    assert_stdout_matches_golden(&output, &golden);
}

#[test]
fn health_github_preserves_error_warning_and_notice_severity() {
    let report = write_report(&health_report());
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.path().to_str().expect("report path"),
            "--format",
            "github",
            "--max-annotations",
            "3",
        ])
        .output()
        .expect("run health");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("GitHub annotations are UTF-8");
    let annotations = stdout.lines().collect::<Vec<_>>();
    assert_eq!(annotations.len(), 3);
    assert!(annotations[0].starts_with("::error file=src/a%2Cb%25file.rs,"));
    assert!(annotations[1].starts_with("::warning file=src/second.rs,"));
    assert!(annotations[2].starts_with("::notice file=src/watchlist.rs,"));
}

#[test]
fn check_github_escapes_hostile_tracked_paths_as_one_command() {
    let mut report = health_report();
    report["files"][0]["path"] = json!("src/hostile.rs\n::warning file=owned.rs::owned:message,%");
    let report = write_report(&report);
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "check",
            "--report",
            report.path().to_str().expect("report path"),
            "--format",
            "github",
        ])
        .output()
        .expect("run check GitHub renderer");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).expect("GitHub annotations are UTF-8");
    let annotations = stdout.lines().collect::<Vec<_>>();
    assert_eq!(annotations.len(), 1);
    assert_eq!(
        annotations[0],
        "::error file=src/hostile.rs%0A%3A%3Awarning file=owned.rs%3A%3Aowned%3Amessage%2C%25::Git Slop context=critical slop=high score=88.0"
    );
}

#[test]
fn health_json_derives_the_persisted_contract_for_explicit_legacy_reports() {
    let report = write_report(&health_report());
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.path().to_str().expect("report path"),
            "--format",
            "json",
        ])
        .output()
        .expect("run health");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("health JSON");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["command"], "health");
    assert_eq!(payload["health"]["file_band_counts"]["budget_exceeded"], 1);
    assert_eq!(payload["health"]["file_band_counts"]["warning"], 1);
    assert_eq!(payload["health"]["findings"][0]["path"], "src/a,b%file.rs");
}

#[test]
fn report_missing_and_check_failure_keep_their_exit_codes() {
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            "definitely-not-a-report.json",
            "--format",
            "json",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Report not found. Searched: definitely-not-a-report.json",
        ));

    let report = write_report(&health_report());
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "check",
            "--report",
            report.path().to_str().expect("report path"),
            "--fail-on-context-band",
            "critical",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Check failed: 1 file records"));
}
