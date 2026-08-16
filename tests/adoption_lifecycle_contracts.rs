use std::fs;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
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
fn config_migrate_is_a_byte_preserving_noop_for_schema_two_or_absent_config() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["config", "migrate"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "built-in defaults already use schema 2",
        ));
    assert!(!repository.path().join(".slop/config.yaml").exists());

    fs::create_dir_all(repository.path().join(".slop")).unwrap();
    let current = "schema_version: 2\n# preserve this comment\n";
    let path = repository.path().join(".slop/config.yaml");
    fs::write(&path, current).unwrap();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["config", "migrate"])
        .assert()
        .success()
        .stdout(predicates::str::contains("was not changed"));
    assert_eq!(fs::read_to_string(path).unwrap(), current);
    assert!(!repository.path().join(".slop/config.yaml.bak").exists());
}

#[test]
fn doctor_and_health_distinguish_current_and_stale_reports() {
    let repository = fixture_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--no-cache", "--persist-unadopted"])
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
        .args(["health"])
        .assert()
        .success()
        .stdout(predicates::str::contains("REPORT SNAPSHOT: stale"))
        .stderr(predicates::str::contains("not the current worktree"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["health", "--format", "markdown"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Stale report snapshot"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["health", "--format", "json"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"current\": false"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["health", "--require-current"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("Report is valid but stale"));
    for arguments in [
        vec!["show", "src/lib.rs", "--require-current"],
        vec!["explain", "--path", "src/lib.rs", "--require-current"],
        vec!["plan", "--path", "src/lib.rs", "--require-current"],
        vec!["list", "findings", "--require-current"],
        vec!["sarif", "--require-current"],
        vec!["html", "--require-current"],
        vec!["baseline", "ensure", "--name", "stale", "--require-current"],
    ] {
        cargo_bin_cmd!("git-slop")
            .current_dir(repository.path())
            .args(arguments)
            .assert()
            .code(2)
            .stderr(predicates::str::contains("Report is valid but stale"));
    }
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
        .args(["find", "--quiet", "--no-cache", "--persist-unadopted"])
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
