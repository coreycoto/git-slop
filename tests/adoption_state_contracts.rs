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

#[test]
fn config_migrate_supports_preview_atomic_apply_and_recovery_backup() {
    let repository = fixture_repository();
    fs::create_dir_all(repository.path().join(".slop")).unwrap();
    let legacy = "history:\n  churn_window_days: 90\n";
    let path = repository.path().join(".slop/config.yaml");
    fs::write(&path, legacy).unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["config", "migrate", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Preview only"));
    assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["config", "migrate"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Recovery backup"));
    assert_eq!(
        fs::read_to_string(repository.path().join(".slop/config.yaml.bak")).unwrap(),
        legacy
    );
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .contains("schema_version: 2")
    );
}

#[test]
fn doctor_and_health_distinguish_current_and_stale_reports() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--no-cache"])
        .assert()
        .success();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["doctor", "--format", "json", "--require-current"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\": \"current\""));
    git(
        repository.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "advance head"],
    );
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["doctor", "--format", "json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"status\": \"stale\""))
        .stdout(predicates::str::contains("head_changed"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["doctor", "--require-current"])
        .assert()
        .code(2);
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["health", "--require-current"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("Report is valid but stale"));
}

#[test]
fn prune_requires_explicit_yes_before_removing_runs() {
    let repository = fixture_repository();
    let runs = repository.path().join(".slop/runs");
    for name in ["2026-08-09T00-00-02Z", "2026-08-09T00-00-01Z"] {
        let path = runs.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("report.json"), name).unwrap();
    }
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["prune", "--keep", "1"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Preview only"));
    assert!(runs.join("2026-08-09T00-00-01Z").is_dir());
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["prune", "--keep", "1", "--yes"])
        .assert()
        .success();
    assert!(!runs.join("2026-08-09T00-00-01Z").exists());
}

#[test]
fn baseline_remove_previews_before_explicit_apply() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--no-cache"])
        .assert()
        .success();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["baseline", "ensure", "--name", "safe-remove"])
        .assert()
        .success();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "baseline",
            "remove",
            "--name",
            "safe-remove",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"preview\": true"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["baseline", "inspect", "--name", "safe-remove"])
        .assert()
        .success();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["baseline", "remove", "--name", "safe-remove", "--yes"])
        .assert()
        .success();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["baseline", "inspect", "--name", "safe-remove"])
        .assert()
        .code(2);
}
