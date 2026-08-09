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
            "git_remote_url": "https://github.com/example/example.git"
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
            "context_band": "healthy",
            "slop_band": "moderate",
            "slop_score": 60.0,
            "reason_codes": []
        }],
        "folders": [],
        "action_queue": []
    })
}

#[test]
fn version_subcommand_preserves_public_shape() {
    let outside_repository = TempDir::new().expect("temporary non-repository directory");
    cargo_bin_cmd!("git-slop")
        .current_dir(outside_repository.path())
        .arg("version")
        .assert()
        .success()
        .stdout(format!("git-slop {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn build_info_reports_version_and_source_identity_as_json() {
    let outside_repository = TempDir::new().expect("temporary non-repository directory");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(outside_repository.path())
        .args(["build-info", "--format", "json"])
        .output()
        .expect("run build-info");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("parse build-info JSON");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["project"], "git-slop");
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
    assert!(payload.get("source_revision").is_some());
    assert!(payload.get("source_dirty").is_some());
}

#[test]
fn find_writes_schema_five_and_all_human_and_machine_surfaces() {
    let repository = committed_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .arg("find")
        .assert()
        .success()
        .stdout(predicate::str::contains("Repository Health"))
        .stdout(predicate::str::contains("Wrote report to"));

    let latest = repository.path().join(".slop/latest");
    for name in ["report.json", "summary.md", "health.md"] {
        assert!(latest.join(name).is_file(), "missing {name}");
    }
    assert!(!latest.join("report.yaml").exists());
    let report: Value = serde_json::from_slice(
        &fs::read(latest.join("report.json")).expect("read generated report"),
    )
    .expect("parse generated report");
    assert_eq!(report["schema_version"], 5);
    assert_eq!(report["files"].as_array().map(Vec::len), Some(3));
    assert!(report["health"]["findings"].is_array());
    assert!(report["repo"]["head_sha"].as_str().is_some());
    assert_eq!(
        report["overlays"]["organization_health"]["analysis_status"],
        "experimental"
    );
    assert_eq!(
        report["overlays"]["organization_health"]["analysis_version"],
        2
    );
    assert_eq!(
        report["overlays"]["organization_health"]["relationships"]["analysis_version"],
        2
    );
    assert_eq!(
        report["overlays"]["organization_health"]["clusters"]["analysis_version"],
        2
    );
    for key in [
        "duplicate_neighborhoods",
        "near_duplicate_neighborhoods",
        "temporal_coupling_edges",
        "lexical_affinity_edges",
        "boundary_leakage_edges",
    ] {
        assert!(report["overlays"]["organization_health"]["relationships"][key].is_array());
    }
    for key in [
        "duplicate_sets",
        "scattered_concepts",
        "boundary_leakage_clusters",
        "consolidation_candidates",
    ] {
        assert!(report["overlays"]["organization_health"]["clusters"][key].is_array());
    }
    for overlay in [
        "organization_health",
        "verification",
        "navigation",
        "blast_radius",
        "stewardship",
        "concept_dispersion",
    ] {
        assert_eq!(
            report["overlays"][overlay]["analysis_status"],
            "experimental"
        );
        assert_eq!(report["overlays"][overlay]["analysis_version"], 2);
    }
    assert!(report["overlays"]["concept_dispersion"]["findings"].is_array());

    let files = report["files"].as_array().expect("file records");
    let total_tokens: u64 = files
        .iter()
        .map(|file| file["tokens"].as_u64().expect("tokens"))
        .sum();
    let line_weights: Vec<u64> = files
        .iter()
        .map(|file| file["line_churn_window"].as_u64().expect("line churn"))
        .collect();
    let total_line_weight: u64 = line_weights.iter().sum();
    let entropy: f64 = line_weights
        .iter()
        .filter(|weight| **weight > 0)
        .map(|weight| {
            let probability = *weight as f64 / total_line_weight as f64;
            -probability * probability.log2()
        })
        .sum();
    let total_hunks: u64 = line_weights
        .iter()
        .map(|line_delta| (*line_delta).max(1).div_ceil(20))
        .sum();
    let expected_diffusion = 0.35 * (3_f64.ln_1p() / 25_f64.ln()).min(1.0)
        + 0.25 * ((total_hunks as f64).ln_1p() / 50_f64.ln()).min(1.0)
        + 0.20 * (2_f64.ln_1p() / 10_f64.ln()).min(1.0)
        + 0.20 * (entropy / 3.0).min(1.0);

    for file in files {
        let tokens = file["tokens"].as_u64().expect("tokens");
        let path = file["path"].as_str().expect("path");
        let folder_token_count: u64 = files
            .iter()
            .filter(|candidate| {
                let candidate_path = candidate["path"].as_str().expect("candidate path");
                candidate_path.rsplit_once('/').map(|pair| pair.0)
                    == path.rsplit_once('/').map(|pair| pair.0)
            })
            .map(|candidate| candidate["tokens"].as_u64().expect("candidate tokens"))
            .sum();
        let load = &file["costs"]["load"];
        assert_eq!(load["file_token_count"], tokens);
        assert_eq!(load["folder_token_count"], folder_token_count);
        assert_close(
            load["top_file_share"].as_f64().expect("top file share"),
            round6(tokens as f64 / folder_token_count as f64),
        );
        assert_eq!(load["top_3_file_share"], 1.0);
        assert_close(
            load["token_concentration_ratio"]
                .as_f64()
                .expect("token concentration"),
            round6(tokens as f64 / total_tokens as f64),
        );

        let coordination = &file["costs"]["coordination"];
        let expected_cross_folder_ratio = if path == "README.md" { 1.0 } else { 0.5 };
        assert_eq!(coordination["files_touched_per_change"], 3.0);
        assert_eq!(coordination["folders_touched_per_change"], 2.0);
        assert_eq!(coordination["edit_hunks_per_change"], 1.0);
        assert_eq!(coordination["cochange_degree"], 2);
        assert_eq!(coordination["cochange_centrality"], 1.0);
        assert_eq!(
            coordination["cross_folder_cochange_ratio"],
            expected_cross_folder_ratio
        );
        assert_eq!(coordination["cochange_pagerank"], 0.333333);
        assert_close(
            coordination["change_diffusion"]
                .as_f64()
                .expect("change diffusion"),
            round6(expected_diffusion),
        );
        assert_close(
            coordination["coordination_pressure"]
                .as_f64()
                .expect("coordination pressure"),
            round6((0.5 * expected_diffusion + 0.3 + 0.2 * expected_cross_folder_ratio).min(1.0)),
        );
    }
    assert!(
        fs::read_to_string(latest.join("health.md"))
            .expect("health report")
            .contains("# Repository Health")
    );
}

#[test]
fn find_rejects_escaping_missing_and_empty_scopes() {
    let repository = committed_repository();
    for scope in ["../outside", "/absolute/path", "missing"] {
        cargo_bin_cmd!("git-slop")
            .current_dir(repository.path())
            .args(["find", "--quiet", "--scope", scope])
            .assert()
            .failure();
    }

    fs::create_dir_all(repository.path().join("empty")).expect("empty directory");
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--scope", "empty"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("selected no tracked paths"));
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet", "--scope", "empty", "--allow-empty-scope"])
        .assert()
        .success();
}

#[test]
fn report_validate_rejects_a_missing_canonical_nested_field() {
    let repository = committed_repository();
    cargo_bin_cmd!("git-slop")
        .current_dir(repository.path())
        .args(["find", "--quiet"])
        .assert()
        .success();
    let report_path = repository.path().join(".slop/latest/report.json");
    cargo_bin_cmd!("git-slop")
        .args([
            "report",
            "validate",
            report_path.to_str().expect("report path"),
        ])
        .assert()
        .success();

    let mut report: Value =
        serde_json::from_slice(&fs::read(&report_path).expect("report")).expect("report JSON");
    report["files"][0]
        .as_object_mut()
        .expect("file")
        .remove("content_fingerprint");
    fs::write(
        &report_path,
        serde_json::to_vec(&report).expect("serialize"),
    )
    .expect("write invalid report");
    cargo_bin_cmd!("git-slop")
        .args([
            "report",
            "validate",
            report_path.to_str().expect("report path"),
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("content_fingerprint"));
}

#[test]
fn relationship_plan_matches_json_and_text_goldens() {
    let report = fixture("relationship_focused_report.json");
    let relationship = "near_duplicate_neighborhood-35e7fad1c4e0";

    let json_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "plan",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--format",
            "json",
        ])
        .output()
        .expect("run plan");
    assert!(json_output.status.success());
    let actual: Value = serde_json::from_slice(&json_output.stdout).expect("plan JSON");
    let expected: Value = serde_json::from_slice(
        &fs::read(fixture("relationship_focused_plan.json")).expect("golden JSON"),
    )
    .expect("parse golden JSON");
    assert_eq!(actual, expected);

    let text_golden = fixture("relationship_focused_plan.txt");
    let text_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "plan",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--format",
            "text",
        ])
        .output()
        .expect("run text plan");
    assert_stdout_matches_golden(&text_output, &text_golden);
}

#[test]
fn show_preserves_same_id_memberships_across_cluster_kinds() {
    let report = fixture("relationship_focused_report.json");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "show",
            "src/consumer_toolkit/github/current_repo.py",
            "--report",
            report.to_str().expect("fixture path"),
            "--format",
            "json",
        ])
        .output()
        .expect("run show");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("show JSON");
    let memberships = payload["cluster_memberships"]
        .as_array()
        .expect("cluster memberships");
    let matching_kinds: Vec<&str> = memberships
        .iter()
        .filter(|membership| membership["id"] == "duplicate_set-ce293b441009")
        .filter_map(|membership| membership["kind"].as_str())
        .collect();
    assert_eq!(matching_kinds, ["duplicate_set", "consolidation_candidate"]);
}

#[test]
fn relationship_explain_matches_rich_text_golden_without_changing_json_contract() {
    let report = fixture("relationship_focused_report.json");
    let relationship = "near_duplicate_neighborhood-35e7fad1c4e0";
    let text_golden = fixture("relationship_focused_explain.txt");

    let text_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--format",
            "text",
        ])
        .output()
        .expect("run text relationship explain");
    assert_stdout_matches_golden(&text_output, &text_golden);

    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--relationship",
            relationship,
            "--format",
            "json",
        ])
        .output()
        .expect("run relationship explain");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("relationship explain JSON");
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(
        payload["target"]["relationship_kind"],
        "near_duplicate_neighborhood"
    );
    assert_eq!(
        payload["cost_summary"]["source"]["costs"]["load"]["file_token_count"],
        490
    );
    assert_eq!(
        payload["overlay_summary"]["target_overlays"]["concept_dispersion"]["concept_dispersion_pressure"],
        1.0
    );
}

#[test]
fn cluster_explain_uses_cluster_kind_and_matches_rich_text_golden() {
    let report = fixture("relationship_focused_report.json");
    let cluster = "duplicate_set-ce293b441009";
    let text_golden = fixture("cluster_focused_explain.txt");

    let text_output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--cluster",
            cluster,
            "--format",
            "text",
        ])
        .output()
        .expect("run text cluster explain");
    assert_stdout_matches_golden(&text_output, &text_golden);

    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "explain",
            "--report",
            report.to_str().expect("fixture path"),
            "--cluster",
            cluster,
            "--format",
            "json",
        ])
        .output()
        .expect("run cluster explain");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("cluster explain JSON");
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(payload["target"]["cluster_kind"], "duplicate_set");
    assert_eq!(
        payload["target"]["candidate_type"],
        "consolidate_duplicate_knowledge"
    );
    assert_eq!(payload["target"]["top_level_roots"], json!(["src"]));
    assert_eq!(
        payload["cost_summary"]["member_hotspots"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[test]
fn health_github_is_advisory_capped_actionable_and_escaped() {
    let report = write_report(&health_report());
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.path().to_str().expect("report path"),
            "--format",
            "github",
            "--max-annotations",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "::error file=src/a%2Cb%25file.rs,title=Context budget exceeded::",
        ))
        .stdout(predicate::str::contains(
            "Next: git-slop explain --path 'src/a,b%25file.rs'",
        ))
        .stdout(predicate::str::contains("src/second.rs").not());
}

#[test]
fn health_markdown_matches_folder_guidance_golden() {
    let report = fixture("health_folder_guidance_report.json");
    let golden = fixture("health_folder_guidance.md");
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.to_str().expect("fixture path"),
            "--format",
            "markdown",
        ])
        .output()
        .expect("run health Markdown");

    assert_stdout_matches_golden(&output, &golden);
}

#[test]
fn health_github_preserves_error_warning_and_notice_severity() {
    let report = write_report(&health_report());
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.path().to_str().expect("report path"),
            "--format",
            "github",
            "--max-annotations",
            "3",
        ])
        .output()
        .expect("run health");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("GitHub annotations are UTF-8");
    let annotations = stdout.lines().collect::<Vec<_>>();
    assert_eq!(annotations.len(), 3);
    assert!(annotations[0].starts_with("::error file=src/a%2Cb%25file.rs,"));
    assert!(annotations[1].starts_with("::warning file=src/second.rs,"));
    assert!(annotations[2].starts_with("::notice file=src/watchlist.rs,"));
}

#[test]
fn health_json_derives_the_persisted_contract_for_explicit_legacy_reports() {
    let report = write_report(&health_report());
    let output = cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            report.path().to_str().expect("report path"),
            "--format",
            "json",
        ])
        .output()
        .expect("run health");
    assert!(output.status.success());
    let payload: Value = serde_json::from_slice(&output.stdout).expect("health JSON");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["command"], "health");
    assert_eq!(payload["health"]["file_band_counts"]["budget_exceeded"], 1);
    assert_eq!(payload["health"]["file_band_counts"]["warning"], 1);
    assert_eq!(payload["health"]["findings"][0]["path"], "src/a,b%file.rs");
}

#[test]
fn report_missing_and_check_failure_keep_their_exit_codes() {
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "health",
            "--report",
            "definitely-not-a-report.json",
            "--format",
            "json",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Report not found: definitely-not-a-report.json",
        ));

    let report = write_report(&health_report());
    cargo_bin_cmd!("git-slop")
        .current_dir(manifest_dir())
        .args([
            "check",
            "--report",
            report.path().to_str().expect("report path"),
            "--fail-on-context-band",
            "critical",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Check failed: 1 file records"));
}
