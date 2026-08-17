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

fn assert_matches_schema(value: &Value, schema_name: &str) {
    let schema: Value = serde_json::from_slice(
        &fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("schemas")
                .join(schema_name),
        )
        .expect("read schema"),
    )
    .expect("parse schema");
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("valid schema");
    if let Some(error) = validator.iter_errors(value).next() {
        panic!(
            "value does not match {schema_name} at {}: {error}",
            error.instance_path()
        );
    }
}

#[test]
fn init_json_is_a_stable_receipt_and_stages_every_repository_owned_slop_file() {
    let repository = fixture_repository();
    fs::create_dir_all(repository.path().join(".slop")).unwrap();
    fs::write(
        repository.path().join(".slop/policies.yaml"),
        "schema_version: 1\npacks: []\n",
    )
    .unwrap();
    fs::write(repository.path().join(".slop/policy-lock.json"), "{}\n").unwrap();

    let output = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--format", "json"])
        .output()
        .expect("init JSON");
    assert!(output.status.success());
    let receipt: Value = serde_json::from_slice(&output.stdout).expect("receipt JSON");
    assert_matches_schema(&receipt, "init-1.json");
    assert_eq!(receipt["mode"], "initialize");
    assert_eq!(receipt["status"], "initialized");
    assert_eq!(
        receipt["staging"]["paths"],
        json!([
            ".slop/config.yaml",
            ".slop/.gitignore",
            ".slop/policies.yaml",
            ".slop/policy-lock.json"
        ])
    );
    assert_eq!(
        receipt["staging"]["command"],
        "git add .slop/config.yaml .slop/.gitignore .slop/policies.yaml .slop/policy-lock.json"
    );

    let check = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--check", "--format", "json"])
        .output()
        .expect("check JSON");
    assert!(check.status.success());
    let check: Value = serde_json::from_slice(&check.stdout).expect("check receipt");
    assert_matches_schema(&check, "init-1.json");
    assert_eq!(check["applied"], false);
    assert_eq!(check["changed_paths"], json!([]));
}

#[test]
fn init_json_format_has_parser_and_runtime_error_parity() {
    let repository = fixture_repository();
    let parser = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--format", "json", "--unknown-option"])
        .output()
        .expect("parser error");
    assert!(!parser.status.success());
    let parser: Value = serde_json::from_slice(&parser.stderr).expect("parser error JSON");
    assert_eq!(parser["error"]["code"], "parser_error");
    assert_eq!(parser["error"]["command"], "init");

    fs::write(
        repository.path().join(".slop"),
        "blocks the state directory\n",
    )
    .unwrap();
    let runtime = cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["init", "--format", "json"])
        .output()
        .expect("runtime error");
    assert!(!runtime.status.success());
    let runtime: Value = serde_json::from_slice(&runtime.stderr).expect("runtime error JSON");
    assert_eq!(runtime["error"]["command"], "init");
    assert_eq!(runtime["error"]["kind"], "io");
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
