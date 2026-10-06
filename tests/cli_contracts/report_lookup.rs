#[test]
fn explain_distinguishes_compact_omission_from_an_absent_path_without_rescanning() {
    let temporary = TempDir::new().expect("temporary directory");
    let repository = temporary.path().join("repository");
    fs::create_dir(&repository).expect("create repository");
    git(&repository, &["init", "--quiet"]);
    for index in 0..300 {
        fs::write(
            repository.join(format!("file-{index:03}.ts")),
            format!("export const value{index} = {index};\n"),
        )
        .expect("write tracked file");
    }
    git(&repository, &["add", "."]);
    git(
        &repository,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );

    for profile in ["compact", "standard"] {
        let output = temporary.path().join(profile);
        cargo_bin_cmd!("git-slop")
            .args(["--repo"])
            .arg(&repository)
            .args([
                "find",
                "--report-profile",
                profile,
                "--quiet",
                "--no-progress",
                "--no-cache",
                "--state-dir",
            ])
            .arg(temporary.path().join(format!("{profile}-state")))
            .arg("--output-dir")
            .arg(&output)
            .assert()
            .success();
        let report = output.join("latest/report.json");
        let original = fs::read(&report).expect("report bytes");
        let payload = read_json(&report);
        assert_eq!(
            payload["compare_index"]["files"].as_array().map(Vec::len),
            Some(300)
        );
        assert_eq!(
            payload["files"].as_array().map(Vec::len),
            Some(if profile == "compact" { 250 } else { 300 })
        );
        let explained = cargo_bin_cmd!("git-slop")
            .arg("--repo")
            .arg(&repository)
            .args(["explain", "--report"])
            .arg(&report)
            .args([
                "--require-current",
                "--path",
                "file-250.ts",
                "--format",
                "json",
            ])
            .output()
            .expect("explain inventoried path");
        if profile == "compact" {
            assert_eq!(explained.status.code(), Some(2));
            let stderr = String::from_utf8_lossy(&explained.stderr);
            assert!(
                stderr.contains("Detailed evidence for 'file-250.ts' was omitted"),
                "{stderr}"
            );
            assert!(
                stderr.contains("git slop find --report-profile standard"),
                "{stderr}"
            );
            assert!(!stderr.contains("No record found"), "{stderr}");
        } else {
            assert!(
                explained.status.success(),
                "{}",
                String::from_utf8_lossy(&explained.stderr)
            );
            let explained: Value = serde_json::from_slice(&explained.stdout).expect("explain JSON");
            assert_eq!(explained["target"]["path"], "file-250.ts");
        }
        cargo_bin_cmd!("git-slop")
            .arg("--repo")
            .arg(&repository)
            .args(["explain", "--report"])
            .arg(&report)
            .args(["--require-current", "--path", "missing.ts"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("No record found for 'missing.ts'"));
        assert_eq!(fs::read(&report).expect("unchanged report"), original);
    }
    let status = Command::new("git")
        .current_dir(&repository)
        .args(["status", "--porcelain"])
        .output()
        .expect("repository status");
    assert!(status.status.success());
    assert!(
        status.stdout.is_empty(),
        "find/explain changed the fixture worktree"
    );
}
