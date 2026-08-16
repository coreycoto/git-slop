use std::fs;
use std::path::PathBuf;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::{Value, json};
use tempfile::{NamedTempFile, TempDir};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(path: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/reports").join(path)
}

fn write_report(report: &Value) -> NamedTempFile {
    let file = NamedTempFile::new().expect("temporary report");
    serde_json::to_writer_pretty(file.as_file(), report).expect("write report");
    file
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 0.000_001,
        "expected {expected}, got {actual}"
    );
}

fn assert_stdout_matches_golden(output: &std::process::Output, golden: &std::path::Path) {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = std::str::from_utf8(&output.stdout).expect("command stdout is UTF-8");
    if std::env::var_os("UPDATE_GIT_SLOP_GOLDENS").is_some() {
        fs::write(golden, actual).expect("update text golden");
    }
    let expected = fs::read_to_string(golden).expect("read text golden");
    assert_eq!(actual.replace("\r\n", "\n"), expected.replace("\r\n", "\n"));
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn git(repository: &TempDir, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository.path())
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

fn committed_repository() -> TempDir {
    let repository = TempDir::new().expect("temporary repository");
    git(&repository, &["init"]);
    git(&repository, &["config", "user.name", "Git Slop Test"]);
    git(
        &repository,
        &["config", "user.email", "git-slop@example.invalid"],
    );
    fs::create_dir(repository.path().join("src")).expect("source directory");
    fs::write(
        repository.path().join("src/lib.rs"),
        "pub fn repository_health() -> &'static str {\n    \"healthy\"\n}\n",
    )
    .expect("source file");
    fs::write(
        repository.path().join("src/main.rs"),
        "fn main() {\n    println!(\"healthy\");\n}\n",
    )
    .expect("source file");
    fs::write(
        repository.path().join("README.md"),
        "# Fixture\n\nA small committed repository.\n",
    )
    .expect("readme");
    git(&repository, &["add", "."]);
    git(
        &repository,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Create fixture",
        ],
    );
    repository
}

fn health_report() -> Value {
    json!({
        "schema_version": 4,
        "generated_at": "2026-07-30T09:48:24Z",
        "repo": {
            "repo_name": "example",
            "branch": "main",
            "head_commit": "0123456789abcdef",
            "git_remote_url": "https://github.com/example/example.git",
            "worktree_clean": true
        },
        "diagnostics": {
            "analysis": {"analysis_status": "complete"}
        },
        "collection_metadata": {
            "files": {"total": 3, "returned": 3, "limit": null, "truncated": false},
            "folders": {"total": 0, "returned": 0, "limit": null, "truncated": false}
        },
        "config": {
            "tokenization": {
                "context_bands": {
                    "compact_max_tokens": 3072,
                    "healthy_max_tokens": 8000,
                    "warning_max_tokens": 10000
                }
            },
            "health": {
                "folder_bands": {
                    "compact_max_direct_tokens": 31999,
                    "healthy_max_direct_tokens": 128000,
                    "warning_max_direct_tokens": 256000,
                    "warning_max_direct_files": 17,
                    "refactor_required_max_direct_files": 37
                }
            }
        },
        "stats": {},
        "summary": {},
        "overlays": {},
        "health": {},
        "files": [{
            "path": "src/a,b%file.rs",
            "profile": "agent_context",
            "classification": "source",
            "language": "Rust",
            "lines": 100,
            "code_lines": 80,
            "comment_lines": 10,
            "blank_lines": 10,
            "tokens": 12000,
            "analysis_status": "analyzed",
            "context_band": "critical",
            "slop_band": "critical",
            "slop_score": 88.0,
            "reason_codes": ["critical_token_cost"]
        }, {
            "path": "src/second.rs",
            "profile": "agent_context",
            "classification": "source",
            "language": "Rust",
            "lines": 50,
            "code_lines": 40,
            "comment_lines": 5,
            "blank_lines": 5,
            "tokens": 9000,
            "analysis_status": "analyzed",
            "context_band": "warning",
            "slop_band": "high",
            "slop_score": 70.0,
            "reason_codes": ["high_token_cost"]
        }, {
            "path": "src/watchlist.rs",
            "profile": "agent_context",
            "classification": "source",
            "language": "Rust",
            "lines": 30,
            "code_lines": 24,
            "comment_lines": 3,
            "blank_lines": 3,
            "tokens": 6000,
            "analysis_status": "analyzed",
            "context_band": "healthy",
            "slop_band": "moderate",
            "slop_score": 60.0,
            "reason_codes": []
        }],
        "folders": [],
        "action_queue": []
    })
}

include!("cli_runtime/group_1.rs");
include!("cli_runtime/group_2.rs");
include!("cli_runtime/group_3.rs");
