mod assembly;
mod render;
mod support;
mod write;

pub use render::render_terminal;
pub use write::{
    load_report, load_report_with_legacy, migrate_legacy_report, schema, validation_violations,
    write_json_atomically, write_report_bundle,
};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::config;
    use crate::model::{
        Analysis, HealthRollup, OrganizationAnalysis, RepoMetadata, ScopeIdentity, SkippedCounts,
    };

    fn analysis(root: &Path) -> Analysis {
        Analysis {
            output_root: root.join(".slop"),
            report_profile: "standard".to_string(),
            compression: "none".to_string(),
            repo: RepoMetadata {
                repo_name: "fixture".to_string(),
                repo_root: root.display().to_string(),
                repository_id: Some("remote:github.com/example/fixture".to_string()),
                repository_identity_source: Some("normalized_remote".to_string()),
                branch: Some("main".to_string()),
                head_commit: Some("abc123".to_string()),
                head_commit_timestamp: Some("2026-07-29T08:00:00Z".to_string()),
                git_remote_url: Some("git@github.com:example/fixture.git".to_string()),
                is_shallow: false,
                detached_head: false,
                worktree_clean: true,
                staged_change_count: 0,
                modified_tracked_file_count: 0,
                untracked_file_count: 0,
                worktree_state_digest: String::new(),
                analyzed_content_digest: None,
            },
            config: crate::config::default_config(),
            generated_at: "2026-07-30T10:11:12Z".to_string(),
            analyzed_revision_at: Some("2026-07-29T08:00:00Z".to_string()),
            skipped: SkippedCounts::default(),
            tracked_file_count: 0,
            scope: ScopeIdentity {
                mode: "repository".to_string(),
                path: None,
                selected_path_count: 0,
                selected_path_digest: String::new(),
            },
            files: vec![],
            folders: vec![],
            organization: OrganizationAnalysis::default(),
            action_queue: vec![],
            observation_feed: vec![],
            diagnostics: json!({}),
        }
    }

    #[test]
    fn report_keeps_generation_and_revision_timestamps_distinct() {
        let root = tempdir().expect("temporary directory");
        let analysis = analysis(root.path());
        let report = assembly::assemble_report(&analysis, &HealthRollup::default());
        assert_eq!(report["generated_at"], "2026-07-30T10:11:12Z");
        assert_eq!(report["analyzed_revision_at"], "2026-07-29T08:00:00Z");
        assert!(report["repo"].get("head_commit").is_none());
        assert_eq!(report["repo"]["head_sha"], "abc123");
        assert_eq!(
            report["repo"]["remote_url"],
            "git@github.com:example/fixture.git"
        );
    }

    #[test]
    fn bundle_writes_compatibility_and_health_surfaces_atomically() {
        let root = tempdir().expect("temporary directory");
        let analysis = analysis(root.path());
        let result =
            write_report_bundle(&analysis, &HealthRollup::default()).expect("report bundle");
        assert!(result.report_json.is_file());
        assert!(!result.report_yaml.exists());
        assert!(result.summary_md.is_file());
        assert!(result.health_md.is_file());
        assert!(
            fs::read_to_string(&result.summary_md)
                .expect("summary")
                .starts_with("# Git Slop Summary")
        );
        assert!(
            fs::read_to_string(&result.health_md)
                .expect("health")
                .starts_with("# Repository Health")
        );
        assert_eq!(
            result.report,
            load_report(&result.report_json).expect("report")
        );
        let run_directories = fs::read_dir(config::runs_dir(root.path()))
            .expect("runs")
            .count();
        assert_eq!(run_directories, 1);
    }

    #[test]
    fn compatibility_summary_is_bounded_and_actionable() {
        let mut analysis = analysis(Path::new("/tmp/fixture"));
        analysis.action_queue = (0..40)
            .map(|index| {
                json!({
                    "path": format!("src/file_{index}.rs"),
                    "slop_band": "moderate",
                    "context_band": "healthy",
                    "slop_score": 50.0,
                    "tokens": 4_000,
                    "age_days": 1,
                    "revisions_window": 2,
                    "churn_pressure": 0.5,
                    "reason_codes": ["high_revision_frequency"],
                    "is_pure_context_hotspot": false
                })
            })
            .collect();
        let report = assembly::assemble_report(&analysis, &HealthRollup::default());
        let summary = render::render_compatibility_summary(&report);
        assert_eq!(
            summary.matches("| [src/file_").count(),
            super::render::DEFAULT_SUMMARY_LIMIT
        );
        assert!(summary.contains("git slop explain --path"));
        assert!(summary.contains("changes frequently"));
    }

    #[test]
    fn published_report_schema_matches_the_runtime_contract() {
        let published: serde_json::Value =
            serde_json::from_str(include_str!("../schemas/report-5.json"))
                .expect("published report schema");
        assert_eq!(published, super::write::schema());
    }
}
