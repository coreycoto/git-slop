use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use assert_cmd::cargo::cargo_bin_cmd;
use chrono::{Duration, Utc};
use serde_json::Value;
use tempfile::{TempDir, tempdir};

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git(root: &Path, args: &[&str]) {
    success(
        Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap(),
    );
}

// Default 60/20/20 scoring: large context, old first appearance, and five
// recent revisions with real churn. No score, band, or threshold overrides.
fn critical_repository() -> TempDir {
    let root = tempdir().unwrap();
    git(root.path(), &["init", "--quiet"]);
    git(
        root.path(),
        &["config", "user.name", "Critical report fixture"],
    );
    git(
        root.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(root.path(), &["config", "commit.gpgsign", "false"]);
    fs::create_dir(root.path().join("src")).unwrap();
    for revision in 0..=5 {
        let source: String = (0..1_500)
            .map(|index| format!("pub fn value_{index}() -> usize {{ {revision} }}\n"))
            .collect();
        fs::write(root.path().join("src/critical.rs"), source).unwrap();
        git(root.path(), &["add", "."]);
        let date = if revision == 0 {
            "2020-01-01T00:00:00Z".to_string()
        } else {
            (Utc::now() - Duration::days(6 - revision)).to_rfc3339()
        };
        success(
            Command::new("git")
                .current_dir(root.path())
                .args(["commit", "--quiet", "-m", "fixture revision"])
                .env("GIT_AUTHOR_DATE", &date)
                .env("GIT_COMMITTER_DATE", &date)
                .output()
                .unwrap(),
        );
    }
    root
}

fn assert_critical_indexes(report: &Value) {
    for pointer in [
        "/files",
        "/ranked_files",
        "/action_queue",
        "/policy_index/files",
        "/compare_index/files",
    ] {
        let record = report
            .pointer(pointer)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["path"] == "src/critical.rs")
            .unwrap_or_else(|| panic!("missing critical file in {pointer}"));
        assert_eq!(record["slop_band"], "critical", "{pointer}");
        assert!(record["slop_score"].as_f64().unwrap() >= 85.0, "{pointer}");
    }
}

#[test]
fn critical_reports_round_trip_in_compact_and_standard_without_rescanning() {
    let repository = critical_repository();
    let schema = success(
        cargo_bin_cmd!("git-slop")
            .args(["schema", "report"])
            .output()
            .unwrap(),
    );
    let schema: Value = serde_json::from_slice(&schema.stdout).unwrap();
    let published: Value = serde_json::from_slice(
        &fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas/report-5.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        schema, published,
        "published report schema drifted from runtime"
    );
    let validator = jsonschema::draft202012::options()
        .build(&published)
        .unwrap();
    for profile in ["compact", "standard"] {
        let output = tempdir().unwrap();
        let state = tempdir().unwrap();
        success(
            cargo_bin_cmd!("git-slop")
                .current_dir(repository.path())
                .args([
                    "find",
                    "--quiet",
                    "--no-cache",
                    "--report-profile",
                    profile,
                    "--output-dir",
                    output.path().to_str().unwrap(),
                    "--state-dir",
                    state.path().to_str().unwrap(),
                ])
                .output()
                .unwrap(),
        );
        let report_path = output.path().join("latest/report.json");
        let original = fs::read(&report_path).unwrap();
        let report: Value = serde_json::from_slice(&original).unwrap();
        assert_critical_indexes(&report);
        let errors: Vec<_> = validator
            .iter_errors(&report)
            .map(|error| error.to_string())
            .collect();
        assert!(errors.is_empty(), "{profile}: {errors:?}");
        // All consumers run outside the repository, so a hidden rescan cannot
        // satisfy these checks. The report must remain byte-for-byte intact.
        for args in [
            vec!["report", "validate", report_path.to_str().unwrap()],
            vec![
                "health",
                "--report",
                report_path.to_str().unwrap(),
                "--format",
                "github",
                "--max-annotations",
                "10",
            ],
            vec![
                "explain",
                "--top",
                "3",
                "--report",
                report_path.to_str().unwrap(),
            ],
        ] {
            success(
                cargo_bin_cmd!("git-slop")
                    .current_dir(output.path())
                    .args(args)
                    .output()
                    .unwrap(),
            );
        }
        assert_eq!(original, fs::read(&report_path).unwrap());
    }
}

#[cfg(unix)]
#[test]
fn real_action_publishes_critical_report_after_one_analysis_and_finishes_advisory() {
    use std::os::unix::fs::PermissionsExt;
    let runner = Path::new(env!("CARGO_MANIFEST_DIR")).join("action/runner.mjs");
    // The public crate intentionally excludes maintainer/Action code.
    if !runner.is_file() {
        return;
    }
    let repository = critical_repository();
    let state = tempdir().unwrap();
    let output = state.path().join("output.txt");
    let summary = state.path().join("summary.md");
    let calls = state.path().join("calls.txt");
    let wrapper = state.path().join("git-slop");
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> {}\nexec {} \"$@\"\n",
            quote(&calls),
            quote(cargo_bin_cmd!("git-slop").get_program().as_ref())
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let run = |phase: &str, extra: &[(&str, &str)]| {
        let mut command = Command::new("node");
        command.arg(&runner).arg(phase).current_dir(state.path());
        for (name, _) in std::env::vars_os() {
            let name_str = name.to_string_lossy();
            if ["GITHUB_", "GIT_SLOP_", "FAKE_"]
                .iter()
                .any(|prefix| name_str.starts_with(prefix))
                || ["RUNNER_TEMP", "RUNNER_TOOL_CACHE"].contains(&name_str.as_ref())
            {
                command.env_remove(name);
            }
        }
        command
            .env("RUNNER_TEMP", state.path())
            .env("GITHUB_OUTPUT", &output)
            .env("GITHUB_STEP_SUMMARY", &summary)
            .env("GITHUB_WORKSPACE", repository.path())
            .env("GIT_SLOP_BINARY", &wrapper)
            .env("GIT_SLOP_WORKING_DIRECTORY", ".")
            .env("GIT_SLOP_MODE", "advisory")
            .env("GIT_SLOP_POLICY", "advisory")
            .env("GIT_SLOP_REPORT_PROFILE", "compact")
            .env("GIT_SLOP_ARTIFACT_CONTENTS", "report")
            .envs(extra.iter().copied());
        success(command.output().unwrap())
    };
    let value = |name: &str| {
        let text = fs::read_to_string(&output).unwrap();
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            if line.starts_with(&format!("{name}<<")) {
                return lines.next().unwrap().to_string();
            }
        }
        panic!("missing output {name}: {text}");
    };
    let analysis = run("analyze", &[]);
    assert_eq!(
        value("analysis-exit-code"),
        "0",
        "{}",
        String::from_utf8_lossy(&analysis.stderr)
    );
    assert!(value("health-finding-count").parse::<usize>().unwrap() > 0);
    let report_path = value("report-path");
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_critical_indexes(&report);
    let summary_path = value("summary-path");
    let health_path = value("health-path");
    assert!(Path::new(&summary_path).is_file());
    assert!(
        fs::read_to_string(&summary)
            .unwrap()
            .contains("Repository Health")
    );
    run(
        "annotate",
        &[
            ("GIT_SLOP_REPORT_PATH", &report_path),
            (
                "GIT_SLOP_WORKING_DIRECTORY_RESOLVED",
                repository.path().to_str().unwrap(),
            ),
        ],
    );
    assert!(value("annotation-count").parse::<usize>().unwrap() > 0);
    fs::write(&output, "").unwrap();
    run(
        "artifacts",
        &[
            ("GIT_SLOP_REPORT_PATH", &report_path),
            ("GIT_SLOP_SUMMARY_PATH", &summary_path),
            ("GIT_SLOP_HEALTH_PATH", &health_path),
        ],
    );
    let artifact_paths = fs::read_to_string(&output).unwrap();
    assert!(artifact_paths.contains(&report_path));
    assert!(artifact_paths.contains(&health_path));
    run(
        "finalize",
        &[
            ("GIT_SLOP_ANALYSIS_EXIT_CODE", "0"),
            ("GIT_SLOP_BASELINE_STATUS", "not_evaluated"),
        ],
    );
    assert_eq!(value("policy-exit-code"), "0");
    assert_eq!(value("status"), "advisory");
    assert_eq!(
        fs::read_to_string(calls)
            .unwrap()
            .lines()
            .filter(|call| *call == "find")
            .count(),
        1
    );
}
