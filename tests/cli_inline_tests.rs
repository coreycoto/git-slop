use std::{fs, process::Command};

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;

fn git(repository: &TempDir, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository.path())
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn javascript_inline_test_evidence_does_not_hide_verification_gaps() {
    let repository = TempDir::new().expect("temporary repository");
    git(&repository, &["init"]);
    git(&repository, &["config", "user.name", "Git Slop Test"]);
    git(
        &repository,
        &["config", "user.email", "git-slop@example.invalid"],
    );
    fs::create_dir(repository.path().join("src")).expect("source directory");
    let files = [
        (
            "regex.ts",
            "export const valid = (value: string) => /^ok$/.test(value);\n",
            false,
        ),
        (
            "split.js",
            "export const parts = (value) => value.split(',');\n",
            false,
        ),
        (
            "emit.ts",
            "export const send = (value: string) => emit(value);\n",
            false,
        ),
        (
            "plain.ts",
            "export const valid = (value: string) => value === 'ok';\n",
            false,
        ),
        (
            "string.ts",
            "export const message = `test('fake', () => {})`;\n",
            false,
        ),
        (
            "embedded.ts",
            "test \n ('works', () => { if (1 !== 1) throw new Error('failure'); });\n",
            true,
        ),
    ];
    for (name, source, _) in &files {
        fs::write(repository.path().join("src").join(name), source).expect("source file");
    }
    git(&repository, &["add", "src"]);
    git(
        &repository,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Track commentless JS and TS",
        ],
    );

    let mut scores = Vec::new();
    for profile in ["compact", "standard"] {
        cargo_bin_cmd!("git-slop")
            .current_dir(repository.path())
            .args([
                "find",
                "--persist-unadopted",
                "--no-cache",
                "--no-progress",
                "--report-profile",
                profile,
                "--output-dir",
                "reports",
                "--state-dir",
                "state",
            ])
            .assert()
            .success();
        let report_path = repository.path().join("reports/latest/report.json");
        let report: Value =
            serde_json::from_slice(&fs::read(&report_path).expect("report")).expect("report JSON");
        let mut profile_scores = Vec::new();
        for (name, source, inline) in &files {
            let path = format!("src/{name}");
            let file = report["files"]
                .as_array()
                .expect("files")
                .iter()
                .find(|file| file["path"] == path)
                .expect("source record");
            let verification = &file["overlays"]["verification"];
            assert_eq!(file["comment_lines"], 0, "{profile}: {path}");
            assert_eq!(file["has_inline_tests"], *inline, "{profile}: {path}");
            assert_eq!(
                verification["inline_tests_detected"], *inline,
                "{profile}: {path}"
            );
            assert_eq!(
                verification["mapping_confidence"],
                if *inline { "high" } else { "unavailable" },
                "{profile}: {path}"
            );
            assert_eq!(
                verification["mapping_rationale"],
                if *inline {
                    "inline_test_module"
                } else {
                    "repository_has_no_detected_test_paths"
                },
                "{profile}: {path}"
            );
            assert_eq!(
                verification["evidence_status"],
                if *inline {
                    "evidence_found"
                } else {
                    "no_mapping"
                },
                "{profile}: {path}"
            );
            assert_eq!(
                verification["applicability"], "applicable",
                "{profile}: {path}"
            );
            assert_eq!(
                verification["verification_gap"],
                if *inline { 0.0 } else { 0.8 },
                "{profile}: {path}"
            );
            assert_eq!(
                fs::read_to_string(repository.path().join(&path)).expect("source"),
                *source
            );
            profile_scores.push(file["slop_score"].clone());
        }
        scores.push(profile_scores);
        cargo_bin_cmd!("git-slop")
            .args([
                "report",
                "validate",
                "--report",
                report_path.to_str().expect("report path"),
            ])
            .assert()
            .success();
    }
    assert_eq!(
        scores[0], scores[1],
        "report profiles preserve stable hotspot scores"
    );
}
