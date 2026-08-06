use super::rollup::{build_health_rollup_from_values, distribution, finding_for_file};
use super::*;
use crate::model::FileAnalysis;
use serde_json::{Value, json};

#[test]
fn percentile_interpolates_and_concentration_is_bounded() {
    let stats = distribution(&[100, 200, 300, 400]);
    assert_eq!(stats["p50"], 250.0);
    assert_eq!(stats["p90"], 370.0);
    assert_eq!(stats["max"], 400);
    assert_eq!(stats["top_10_share"], 1.0);
}

#[test]
fn github_blob_links_support_https_and_ssh_remotes() {
    for remote in [
        "https://github.com/coreycoto/git-slop.git",
        "git@github.com:coreycoto/git-slop.git",
    ] {
        let report = json!({
            "repo": {
                "git_remote_url": remote,
                "head_commit": "abc123"
            }
        });
        assert_eq!(
            github_blob_url(&report, "src/a file.rs").as_deref(),
            Some("https://github.com/coreycoto/git-slop/blob/abc123/src/a%20file.rs")
        );
    }
}

#[test]
fn health_numbers_use_stable_human_formatting() {
    assert_eq!(super::render::format_number(1_234.0), "1,234");
    assert_eq!(super::render::format_number(1_234.5), "1,234.50");
    assert_eq!(super::render::format_score(1_234.5), "1,234.5");
    assert_eq!(super::render::format_percent(0.123_55), "12.4%");
    assert_eq!(
        super::render::format_finding_reason(
            "10001 tokens exceed the configured fail threshold",
            10_001,
        ),
        "10,001 tokens exceed the configured fail threshold"
    );
    assert_eq!(
        super::render::folder_next_command("."),
        "git-slop explain --path ."
    );
}

#[test]
fn folder_risks_explain_direct_triggers_and_rank_one_agent_descendant() {
    let report: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reports/health_folder_guidance_report.json"
    ))
    .expect("folder guidance fixture");

    let rendered = render_health_from_report(&report).expect("health renders");

    assert!(rendered.contains("Context/load band"));
    assert!(rendered.contains("Maintenance pressure"));
    assert!(rendered.contains("Review severity"));
    assert!(rendered.contains(r"files: 3 direct files \> 2 healthy ceiling"));
    assert!(rendered.contains(r"tokens: 2,500 direct tokens \> 2,000 healthy ceiling"));
    assert!(rendered.contains(
        r"both: 5 direct files \> 4 warning ceiling; 4,500 direct tokens \> 4,000 warning ceiling"
    ));
    for path in ["src/files-only/", "src/tokens-only/", "src/both/"] {
        assert!(
            rendered.contains(&format!("git-slop explain --path {path}")),
            "missing next command for {path}"
        );
    }
    assert!(rendered.contains("src/files-only/nested/winner.rs"));
    assert!(!rendered.contains("src/files-only/a.rs"));
    assert!(!rendered.contains("src/files-only/generated.json"));
    assert!(rendered.contains("score 1,234.5"));
    assert!(rendered.contains("60.4% of parent"));
}

#[test]
fn report_only_renderer_is_compatible_with_minimal_schema_four() {
    let report = json!({
        "schema_version": 4,
        "generated_at": "2026-07-30T10:00:00Z",
        "repo": {
            "repo_name": "fixture",
            "branch": "main",
            "head_commit": "abc123",
            "git_remote_url": "https://github.com/example/fixture.git"
        },
        "config": {
            "tokenization": {"context_bands": {
                "compact_max_tokens": 3072,
                "healthy_max_tokens": 8000,
                "warning_max_tokens": 10000
            }}
        },
        "stats": {},
        "files": [{
            "path": "src/lib.rs",
            "profile": "agent_context",
            "language": "Rust",
            "classification": "source",
            "tokens": 9001,
            "context_band": "warning",
            "slop_band": "high",
            "slop_score": 70.0,
            "reason_codes": ["high_token_cost"]
        }],
        "folders": []
    });
    let rendered = render_health_from_report(&report).expect("health renders");
    assert!(rendered.starts_with("# Repository Health"));
    assert!(rendered.contains("src/lib.rs"));
    assert!(rendered.contains("Actionable Findings"));
    assert!(rendered.contains("blob/abc123/src/lib.rs"));
}

#[test]
fn markdown_renderer_never_emits_raw_control_characters_from_repository_fields() {
    let report = json!({
        "schema_version": 4,
        "generated_at": "2026-07-30T10:00:00Z",
        "repo": {
            "repo_name": "fixture",
            "branch": "main\n::stop-commands::token",
            "head_commit": "abc123"
        },
        "config": {},
        "stats": {},
        "files": [{
            "path": "src/file.rs\n::error title=forged::message",
            "profile": "agent_context",
            "language": "Rust",
            "classification": "source",
            "tokens": 12_000,
            "context_band": "critical",
            "slop_band": "critical",
            "slop_score": 90.0,
            "reason_codes": ["critical_token_cost"]
        }],
        "folders": []
    });

    let rendered = render_health_from_report(&report).expect("health renders");

    assert!(!rendered.contains("\n::error"));
    assert!(!rendered.contains("\n::stop-commands"));
    assert!(rendered.contains(r"\n::error"));
    assert!(rendered.contains(r"\n::stop-commands"));
}

#[test]
fn folder_health_excludes_data_context_from_direct_metrics() {
    let files = vec![
        json!({
            "path": "src/lib.rs",
            "profile": "agent_context",
            "language": "Rust",
            "tokens": 100,
            "context_band": "compact"
        }),
        json!({
            "path": "data/large.json",
            "profile": "data_context",
            "language": "JSON",
            "tokens": 1_000_000,
            "context_band": "refactor_required"
        }),
    ];
    let folders = vec![
        json!({
            "path": ".",
            "direct_file_count": 0,
            "direct_tokens": 0,
            "tokens": 1_000_100,
            "health_band": "refactor_required"
        }),
        json!({
            "path": "src",
            "direct_file_count": 1,
            "direct_tokens": 100,
            "tokens": 100,
            "health_band": "compact"
        }),
        json!({
            "path": "data",
            "direct_file_count": 1,
            "direct_tokens": 1_000_000,
            "tokens": 1_000_000,
            "health_band": "refactor_required"
        }),
    ];

    let rollup = build_health_rollup_from_values(&files, &folders, &Value::Null);

    assert_eq!(rollup.file_band_counts["compact"], 1);
    assert_eq!(rollup.file_band_counts["refactor_required"], 0);
    assert_eq!(rollup.folder_band_counts["compact"], 2);
    assert_eq!(rollup.folder_band_counts["refactor_required"], 0);
    assert_eq!(rollup.folder_distribution["count"], 2);
    assert_eq!(rollup.folder_distribution["total"], 100);
}

#[test]
fn finding_humanizes_stable_reason_codes() {
    let file = serde_json::to_value(FileAnalysis {
        path: "src/lib.rs".to_string(),
        bytes: 1,
        lines: 1,
        blank_lines: 0,
        code_lines: 1,
        comment_lines: 0,
        language: "Rust".to_string(),
        profile: "agent_context".to_string(),
        classification: "source".to_string(),
        tokens: 10_001,
        context_band: "critical".to_string(),
        context_pressure: 1.0,
        content_fingerprint: String::new(),
        structural_tokens: vec![],
        structural_token_count: 0,
        top_structural_terms: vec![],
        age_days: 1,
        revisions_window: 20,
        recency_weighted_commits: 1.0,
        added_window: 1,
        deleted_window: 0,
        churn_lines_window: 1,
        line_churn_window: 1,
        token_churn_window: 1,
        relative_churn_window: 1.0,
        late_churn_spike: 0.0,
        author_count_window: 1,
        author_entropy: 0.0,
        top_author_share: 1.0,
        days_since_non_bot_edit: Some(1),
        recent_maintainer_diversity: 1,
        age_pressure: 0.0,
        revision_norm: 1.0,
        relative_churn_norm: 1.0,
        churn_pressure: 1.0,
        slop_score: 80.0,
        slop_band: "critical".to_string(),
        reason_codes: vec!["critical_token_cost".to_string()],
        costs: json!({}),
        overlays: json!({}),
    })
    .expect("serialize");
    let finding = finding_for_file(&file, &Value::Null).expect("finding");
    assert_eq!(finding.severity, "error");
    assert_eq!(
        finding.reasons,
        vec!["exceeds the configured context budget"]
    );
}
