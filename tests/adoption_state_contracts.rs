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
fn ephemeral_find_is_git_private_clean_and_prints_a_scan_receipt() {
    let repository = fixture_repository();
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--ephemeral"])
        .output()
        .expect("ephemeral find");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Scan receipt:"), "{stdout}");
    assert!(stdout.contains("cache=0 hit(s)/"), "{stdout}");
    assert!(stdout.contains("profile=standard"), "{stdout}");
    assert!(!repository.path().join(".slop").exists());

    let private = Command::new("git")
        .current_dir(repository.path())
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "git-slop/ephemeral/latest/report.json",
        ])
        .output()
        .unwrap();
    assert!(private.status.success());
    let report = String::from_utf8_lossy(&private.stdout).trim().to_string();
    assert!(std::path::Path::new(&report).is_file(), "{report}");
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
fn doctor_default_bundle_is_git_private_before_adoption() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["doctor", "--bundle"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Wrote redacted diagnostic bundle",
        ));
    assert!(!repository.path().join(".slop").exists());
    let private = Command::new("git")
        .current_dir(repository.path())
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "git-slop/ephemeral/diagnostic-bundle.json",
        ])
        .output()
        .unwrap();
    assert!(private.status.success());
    let bundle = String::from_utf8_lossy(&private.stdout).trim().to_string();
    assert!(std::path::Path::new(&bundle).is_file(), "{bundle}");
    let payload: Value = serde_json::from_slice(&fs::read(bundle).unwrap()).unwrap();
    assert_eq!(payload["privacy"]["absolute_paths_included"], false);
}

#[test]
fn init_promotes_only_current_git_private_first_run_state() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet"])
        .assert()
        .success();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicates::str::contains("Promoted"));
    assert!(repository.path().join(".slop/latest/report.json").is_file());
    assert!(
        repository
            .path()
            .join(".slop/cache/token-v4.sqlite3")
            .is_file()
    );
    assert!(
        !repository
            .path()
            .join(".git/git-slop/ephemeral/latest/report.json")
            .exists()
    );

    let stale_repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(stale_repository.path())
        .args(["find", "--quiet"])
        .assert()
        .success();
    git(
        stale_repository.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "advance head"],
    );
    cargo_bin_cmd!("git-slop")
        .current_dir(stale_repository.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Retained Git-private first-run state",
        ));
    assert!(
        !stale_repository
            .path()
            .join(".slop/latest/report.json")
            .exists()
    );
    assert!(
        stale_repository
            .path()
            .join(".git/git-slop/ephemeral/latest/report.json")
            .is_file()
    );
}

#[test]
fn clean_adopted_repository_stays_clean_and_can_be_baselined() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("init")
        .assert()
        .success();
    git(
        repository.path(),
        &["add", ".slop/config.yaml", ".slop/.gitignore"],
    );
    git(
        repository.path(),
        &["commit", "--quiet", "-m", "adopt git-slop"],
    );
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--no-cache"])
        .assert()
        .success();

    let report: Value = serde_json::from_str(
        &fs::read_to_string(repository.path().join(".slop/latest/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        report.pointer("/repo/worktree_clean"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        report.pointer("/repo/untracked_file_count"),
        Some(&json!(0))
    );
    let status = Command::new("git")
        .current_dir(repository.path())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(status.stdout.is_empty());
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["baseline", "ensure", "--name", "clean-adoption"])
        .assert()
        .success();
}

#[test]
fn init_repair_is_selective_and_checkable() {
    let repository = fixture_repository();
    fs::create_dir_all(repository.path().join(".slop")).unwrap();
    let config = "schema_version: 2\nhistory:\n  churn_window_days: 90\n";
    fs::write(repository.path().join(".slop/config.yaml"), config).unwrap();
    fs::write(
        repository.path().join(".slop/.gitignore"),
        "# keep me\n/latest/\n",
    )
    .unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--check"])
        .assert()
        .code(1)
        .stdout(predicates::str::contains("repair needed"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--repair"])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(repository.path().join(".slop/config.yaml")).unwrap(),
        config
    );
    let ignore = fs::read_to_string(repository.path().join(".slop/.gitignore")).unwrap();
    assert!(ignore.contains("# keep me"));
    for entry in [
        "/latest/",
        "/runs/",
        "/cache/",
        "/scan.lock",
        "/scan.lock.owner",
        "/prompt-packs/",
        "/diagnostic-bundle.json",
        "/config.yaml.bak",
        "/.gitignore.bak",
    ] {
        assert!(ignore.lines().any(|line| line == entry), "missing {entry}");
    }
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--check"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Adoption status: ready"));
}

#[test]
fn init_gitignore_only_never_creates_or_replaces_configuration() {
    let repository = fixture_repository();
    fs::create_dir_all(repository.path().join(".slop")).unwrap();
    fs::write(repository.path().join(".slop/.gitignore"), "# keep me\n").unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--repair", "--gitignore-only"])
        .assert()
        .success()
        .stdout(predicates::str::contains("git add .slop/.gitignore"));
    assert!(!repository.path().join(".slop/config.yaml").exists());
    assert!(
        fs::read_to_string(repository.path().join(".slop/.gitignore"))
            .unwrap()
            .contains("# keep me")
    );
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--check", "--gitignore-only"])
        .assert()
        .success();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--check"])
        .assert()
        .code(1);
}

#[test]
fn doctor_reports_the_exact_safe_adoption_repair_command() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("init")
        .assert()
        .success();
    fs::write(repository.path().join(".slop/.gitignore"), "/latest/\n").unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["doctor", "--format", "json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("adoption_repair_needed"))
        .stdout(predicates::str::contains(
            "git slop init --repair --gitignore-only",
        ));
}
