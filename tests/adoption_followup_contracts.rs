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

#[test]
fn unadopted_find_defaults_to_git_private_storage() {
    let repository = fixture_repository();
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("find")
        .output()
        .expect("default find");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("used Git-private ephemeral storage"),
        "{stdout}"
    );
    assert!(stdout.contains("git slop init"), "{stdout}");
    assert!(!repository.path().join(".slop").exists());
    let status = Command::new("git")
        .current_dir(repository.path())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(
        status.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
}

#[test]
fn empty_repository_doctor_and_estimate_offer_exact_recovery() {
    let repository = tempdir().expect("repository");
    git(repository.path(), &["init", "--quiet"]);
    let doctor = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("doctor")
        .output()
        .expect("doctor");
    assert!(doctor.status.success());
    let stdout = String::from_utf8_lossy(&doctor.stdout);
    assert!(stdout.contains("- status: not ready"), "{stdout}");
    assert!(
        stdout.contains("git slop find --allow-empty-scope"),
        "{stdout}"
    );
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--estimate-only", "--format", "text"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("--allow-empty-scope"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "find",
            "--estimate-only",
            "--allow-empty-scope",
            "--format",
            "text",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Git Slop scan estimate"))
        .stdout(predicates::str::contains("tracked paths: 0"));
}

#[test]
fn cache_prune_requires_explicit_yes_before_removing_entries() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--persist-unadopted"])
        .assert()
        .success();
    let status = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["cache", "status", "--format", "json"])
        .output()
        .expect("cache status");
    let before: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(before["entries"].as_u64().unwrap_or_default() > 0);
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "cache",
            "prune",
            "--max-entries",
            "0",
            "--max-bytes",
            "0",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"dry_run\": true"));
    let status = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["cache", "status", "--format", "json"])
        .output()
        .expect("cache status");
    let previewed: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(previewed["entries"], before["entries"]);
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "cache",
            "prune",
            "--max-entries",
            "0",
            "--max-bytes",
            "0",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Pruned"));
    let status = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["cache", "status", "--format", "json"])
        .output()
        .expect("cache status");
    let after: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(after["entries"], json!(0));
}
