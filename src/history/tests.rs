#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn parses_status_authors_and_rename_edges() {
        let raw = concat!(
            "commit\0abc123\0",
            "10000000\0Example Dev\0dev@example.com\0",
            "R100\0src/old.py\0src/new.py\0",
        );
        assert_eq!(
            parse_status_log(raw),
            vec![StatusCommit {
                commit: "abc123".to_string(),
                timestamp: 10_000_000,
                author: "Example Dev <dev@example.com>".to_string(),
                parents: Vec::new(),
                subject: String::new(),
                changes: vec![StatusChange::Rename {
                    old_path: "src/old.py".to_string(),
                    new_path: "src/new.py".to_string(),
                }],
            }]
        );
    }

    #[test]
    fn parses_numstat_authors_and_rename_paths() {
        let raw = concat!(
            "commit\0def456\0",
            "10000010\0Example Dev\0dev@example.com\0",
            "5\t3\t\0src/old.py\0src/new.py\0",
        );
        assert_eq!(
            parse_numstat_log(raw),
            vec![NumstatCommit {
                commit: "def456".to_string(),
                timestamp: 10_000_010,
                author: "Example Dev <dev@example.com>".to_string(),
                parents: Vec::new(),
                subject: String::new(),
                entries: vec![NumstatEntry {
                    added: 5,
                    deleted: 3,
                    paths: vec!["src/old.py".to_string(), "src/new.py".to_string()],
                }],
            }]
        );
    }

    #[test]
    fn percentile_uses_nearest_rank_ordering() {
        let values: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(nearest_rank_percentile(&values, 0.95), 19.0);
        assert_eq!(nearest_rank_percentile(&values, 0.99), 20.0);
        assert_eq!(nearest_rank_percentile(&[], 0.95), 0.0);
    }

    #[test]
    fn rename_lineage_maps_the_old_name_to_the_current_path() {
        let tracked = BTreeSet::from(["src/current.rs".to_string()]);
        let commits = vec![
            StatusCommit {
                commit: "new-edit".to_string(),
                timestamp: 300,
                author: "Dev <dev@example.com>".to_string(),
                parents: Vec::new(),
                subject: String::new(),
                changes: vec![StatusChange::Path {
                    status: "M".to_string(),
                    path: "src/current.rs".to_string(),
                }],
            },
            StatusCommit {
                commit: "rename".to_string(),
                timestamp: 200,
                author: "Dev <dev@example.com>".to_string(),
                parents: Vec::new(),
                subject: String::new(),
                changes: vec![StatusChange::Rename {
                    old_path: "src/legacy.rs".to_string(),
                    new_path: "src/current.rs".to_string(),
                }],
            },
            StatusCommit {
                commit: "initial".to_string(),
                timestamp: 100,
                author: "Dev <dev@example.com>".to_string(),
                parents: Vec::new(),
                subject: String::new(),
                changes: vec![StatusChange::Path {
                    status: "A".to_string(),
                    path: "src/legacy.rs".to_string(),
                }],
            },
        ];
        assert_eq!(
            first_seen_exact(&tracked, &commits)["src/current.rs"],
            Some(200)
        );
        assert_eq!(
            first_seen_with_lineage(&tracked, &commits)["src/current.rs"],
            Some(100)
        );
    }

    fn git(repo: &Path, args: &[&str], timestamp: Option<i64>, author: Option<(&str, &str)>) {
        let mut command = Command::new("git");
        command.current_dir(repo).args(args);
        if let Some(timestamp) = timestamp {
            let date = format!("@{timestamp}");
            command
                .env("GIT_AUTHOR_DATE", &date)
                .env("GIT_COMMITTER_DATE", &date);
        }
        if let Some((name, email)) = author {
            command
                .env("GIT_AUTHOR_NAME", name)
                .env("GIT_AUTHOR_EMAIL", email)
                .env("GIT_COMMITTER_NAME", name)
                .env("GIT_COMMITTER_EMAIL", email);
        }
        let output = command.output().expect("git should execute");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn computes_window_churn_age_and_authors_deterministically() {
        let temp = TempDir::new().expect("temp directory");
        let repo = temp.path();
        git(repo, &["init", "-q"], None, None);
        git(repo, &["config", "user.name", "History Test"], None, None);
        git(
            repo,
            &["config", "user.email", "history@example.com"],
            None,
            None,
        );
        let now = Utc.with_ymd_and_hms(2026, 7, 30, 0, 0, 0).single().unwrap();
        let initial_timestamp = (now - Duration::days(200)).timestamp();
        let middle_timestamp = (now - Duration::days(100)).timestamp();
        let recent_timestamp = (now - Duration::days(10)).timestamp();

        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/example.rs"), "one\ntwo\n").unwrap();
        git(repo, &["add", "."], None, None);
        git(
            repo,
            &["commit", "-q", "-m", "initial"],
            Some(initial_timestamp),
            Some(("Example Dev", "dev@example.com")),
        );
        fs::write(repo.join("src/example.rs"), "one\ntwo\nthree\n").unwrap();
        git(repo, &["add", "."], None, None);
        git(
            repo,
            &["commit", "-q", "-m", "middle"],
            Some(middle_timestamp),
            Some(("Example Dev", "dev@example.com")),
        );
        fs::write(repo.join("src/example.rs"), "one\ntwo\nthree\nfour\n").unwrap();
        git(repo, &["add", "."], None, None);
        git(
            repo,
            &["commit", "-q", "-m", "recent"],
            Some(recent_timestamp),
            Some(("Build Bot", "bot@example.com")),
        );

        let paths = vec!["src/example.rs".to_string()];
        // A deliberately sparse token count proves stable relative churn is
        // based on changed lines/current lines rather than token churn/tokens.
        let tokens = BTreeMap::from([("src/example.rs".to_string(), 1)]);
        let lines = BTreeMap::from([("src/example.rs".to_string(), 4)]);
        let config = json!({
            "history": {"churn_window_days": 180, "follow_renames": false},
            "stewardship": {"bot_name_markers": ["bot", "[bot]"]},
        });
        let (metrics, commits, baselines) =
            analyze_history(repo, &paths, &tokens, &lines, &config, now).unwrap();
        let metric = &metrics["src/example.rs"];

        assert_eq!(metric.first_seen_timestamp, Some(initial_timestamp));
        assert_eq!(metric.age_days, 200);
        assert_eq!(metric.revisions_window, 2);
        assert_eq!(metric.added_window, 2);
        assert_eq!(metric.deleted_window, 0);
        assert_eq!(metric.line_churn_window, 2);
        assert_eq!(metric.token_churn_window, 2);
        assert_eq!(metric.relative_churn_window, 0.5);
        assert_eq!(metric.late_churn_spike, 0.5);
        assert_eq!(metric.author_count_window, 2);
        assert_eq!(metric.author_entropy, 1.0);
        assert_eq!(metric.top_author_share, 0.5);
        assert_eq!(metric.days_since_non_bot_edit, Some(100));
        assert_eq!(metric.recent_maintainer_diversity, 1);
        assert_eq!(metric.recency_weighted_commits, 0.980769);
        assert_eq!(commits.len(), 2);
        assert!(commits[0].timestamp > commits[1].timestamp);
        assert_eq!(baselines["p95_files_touched"], 1.0);
        assert_eq!(baselines["p95_token_delta_mass"], 1.0);
    }
}
