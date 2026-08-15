use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/reports").join(name)
}

fn git(repository: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read JSON file")).expect("parse JSON file")
}

fn assert_prompt_pack_safety(pack: &Path) -> Value {
    assert!(pack.join("prompt.md").is_file(), "missing prompt.md");
    assert!(pack.join("README.md").is_file(), "missing README.md");
    let manifest = read_json(&pack.join("manifest.json"));
    assert_eq!(manifest["schema_version"], 1);
    for name in ["context.json", "prompt.md", "README.md"] {
        assert_eq!(manifest["files"][name].as_str().map(str::len), Some(64));
    }

    let context = read_json(&pack.join("context.json"));
    assert_eq!(context["prompt_pack_version"], 1);
    assert_eq!(context["report_excerpt"]["schema_version"], 5);
    assert_eq!(
        context["provenance"]["report_sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(context["repository_context"]["included"], false);
    assert_eq!(context["repository_context"]["planning_usable"], false);
    assert_eq!(context["repository_context"]["execution_ready"], false);

    let boundary = context["boundary"].as_str().expect("prompt-pack boundary");
    assert!(boundary.contains("advisory only"));
    assert!(boundary.contains("must not rescore detector truth"));
    assert!(boundary.contains("mutate code, GitHub, or report data"));

    let prompt = fs::read_to_string(pack.join("prompt.md")).expect("read prompt.md");
    assert!(prompt.contains("Use only the facts in context.json"));
    assert!(prompt.contains("do not rescore detector truth"));
    assert!(prompt.contains("instead of inventing context"));

    let readme = fs::read_to_string(pack.join("README.md")).expect("read README.md");
    assert!(readme.contains("must not mutate code, GitHub, or detector truth"));
    assert!(readme.contains("must not rescore detector truth"));

    context
}

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
                "Report not found: {missing_display}"
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

#[test]
fn prompt_pack_rejects_report_metadata_absolute_and_symlink_escape_paths() {
    let repository = TempDir::new().expect("temporary repository");
    git(repository.path(), &["init", "-b", "main"]);
    let outside = TempDir::new().expect("outside directory");
    let secret = outside.path().join("github/current_repo.py");
    let second = outside.path().join("github/shared/current_repo.py");
    fs::create_dir_all(secret.parent().expect("secret parent")).expect("secret directory");
    fs::create_dir_all(second.parent().expect("second parent")).expect("second directory");
    fs::write(&secret, "must not be copied\n").expect("write outside file");
    fs::write(&second, "must not be copied either\n").expect("write outside file");
    fs::create_dir_all(repository.path().join("src")).expect("source directory");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path(),
        repository.path().join("src/consumer_toolkit"),
    )
    .expect("create escaping symlink");
    fs::write(repository.path().join("AGENTS.md"), "# Guidance\n").expect("guidance");

    let report_path = fixture("relationship_focused_report.json");
    let pack = repository.path().join("prompt-pack");

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args([
            "plan",
            "--report",
            report_path.to_str().expect("report path"),
            "--relationship",
            "duplicate_neighborhood-b534129a62cb",
            "--prompt-pack",
            pack.to_str().expect("pack path"),
            "--include-repository-context",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let context = read_json(&pack.join("context.json"));
    let rendered = serde_json::to_string(&context).expect("render context");
    assert!(!rendered.contains("must not be copied"));
    assert!(!rendered.contains(&secret.to_string_lossy().to_string()));
    assert_eq!(context["payload"]["source_report"]["path"], Value::Null);
    assert_eq!(
        context["repository_context"]["truncation"]["source_candidate_count"],
        2
    );
    assert_eq!(context["repository_context"]["execution_ready"], false);
}

#[test]
fn plan_uses_safe_repo_relative_paths_without_exposing_external_local_paths() {
    let report = fixture("relationship_focused_report.json");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "plan",
            "--report",
            report.to_str().expect("report path"),
            "--relationship",
            "near_duplicate_neighborhood-35e7fad1c4e0",
            "--format",
            "json",
        ])
        .output()
        .expect("run plan");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert_eq!(
        payload["source_report"]["path"],
        "tests/fixtures/reports/relationship_focused_report.json"
    );
    assert_eq!(payload["source_report"]["descriptor"], "repo_relative");
    assert!(
        payload["proposed_slices"][0]["baseline_command"]
            .as_str()
            .is_some_and(|command| command
                .contains("tests/fixtures/reports/relationship_focused_report.json"))
    );
}

#[test]
fn init_writes_schema_two_config_ignore_rules_and_state_directories() {
    let repository = TempDir::new().expect("temporary repository");
    git(repository.path(), &["init", "-b", "main"]);

    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Initialized .slop/config.yaml (written).",
        ))
        .stdout(predicate::str::contains(
            "Ensured .slop/latest/, .slop/runs/, and .slop/cache/ exist.",
        ));

    let slop = repository.path().join(".slop");
    let config_path = slop.join("config.yaml");
    let gitignore_path = slop.join(".gitignore");
    assert!(config_path.is_file(), "missing config.yaml");
    assert!(gitignore_path.is_file(), "missing .gitignore");
    for directory in ["latest", "runs", "cache"] {
        assert!(slop.join(directory).is_dir(), "missing {directory}/");
    }

    let config: Value =
        serde_yaml::from_str(&fs::read_to_string(config_path).expect("read generated config.yaml"))
            .expect("parse generated config.yaml");
    assert_eq!(config["schema_version"], 2);
    assert_eq!(
        config.as_object().expect("config object").len(),
        1,
        "init should write the minimal forward-compatible config"
    );

    assert_eq!(
        fs::read_to_string(gitignore_path).expect("read generated .gitignore"),
        "/latest/\n/runs/\n/cache/\n/scan.lock\n/scan.lock.owner\n/prompt-packs/\n/diagnostic-bundle.json\n/config.yaml.bak\n/.gitignore.bak\n"
    );
}
