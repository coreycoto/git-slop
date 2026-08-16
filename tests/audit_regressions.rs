use std::fs;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use tempfile::tempdir;

fn git(root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

fn fixture_repository() -> tempfile::TempDir {
    let root = tempdir().expect("repository");
    git(root.path(), &["init", "--quiet"]);
    git(root.path(), &["config", "user.name", "Fixture"]);
    git(
        root.path(),
        &["config", "user.email", "fixture@example.com"],
    );
    fs::create_dir_all(root.path().join("src")).expect("src");
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn value() -> usize { 1 }\n",
    )
    .expect("source");
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "--quiet", "-m", "fixture"]);
    root
}

fn write_report(repository: &std::path::Path, output: &std::path::Path) -> std::path::PathBuf {
    let state = tempdir().expect("state");
    cargo_bin_cmd!("git-slop")
        .current_dir(repository)
        .args([
            "find",
            "--quiet",
            "--no-cache",
            "--state-dir",
            state.path().to_str().unwrap(),
            "--output-dir",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
    output.join("latest/report.json")
}

include!("audit_regressions/group_1.rs");
include!("audit_regressions/group_2.rs");
include!("audit_regressions/group_3.rs");
