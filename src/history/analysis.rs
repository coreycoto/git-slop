/// Analyze rolling Git history for the current inventory.
///
/// Git logs are walked newest-to-oldest. When rename following is enabled,
/// aliases are expanded after processing each rename commit so older names map
/// back to the current analyzed path. The supplied `now` makes windowing and
/// recency calculations reproducible in tests and cached analyses.
pub fn analyze_history(
    repo_root: &Path,
    analyzed_paths: &[String],
    token_counts: &BTreeMap<String, usize>,
    line_counts: &BTreeMap<String, usize>,
    config: &Value,
    now: DateTime<Utc>,
) -> Result<(BTreeMap<String, HistoryMetrics>, Vec<CommitRecord>, Value)> {
    if !git_has_head(repo_root)? {
        return Ok(empty_result(
            analyzed_paths,
            "not_applicable_unborn_repository",
        ));
    }
    if analyzed_paths.is_empty() {
        return Ok(empty_result(analyzed_paths, "not_applicable_empty_scope"));
    }

    let tracked_paths: BTreeSet<String> = analyzed_paths
        .iter()
        .map(|path| normalized_path(path))
        .collect();
    let follow_renames = pointer_bool(config, "/history/follow_renames", false);
    let max_commits = pointer_u64(config, "/history/max_commits", 10_000);
    let window_days = pointer_u64(
        config,
        "/history/churn_window_days",
        DEFAULT_HISTORY_WINDOW_DAYS,
    );
    let window_days =
        i64::try_from(window_days).context("history.churn_window_days is too large")?;
    let window =
        Duration::try_days(window_days).context("history.churn_window_days is too large")?;
    let cutoff = now
        .checked_sub_signed(window)
        .context("history window precedes Chrono's supported range")?;
    let since = cutoff.to_rfc3339_opts(SecondsFormat::Secs, true);

    let fetch_limit = max_commits.saturating_add(1);
    let mut full_status = load_status_commits(repo_root, None, follow_renames, fetch_limit)?;
    let full_history_cap_reached = full_status.len() as u64 > max_commits;
    full_status.truncate(max_commits as usize);
    let first_seen = if follow_renames {
        first_seen_with_lineage(&tracked_paths, &full_status)
    } else {
        first_seen_exact(&tracked_paths, &full_status)
    };
    let full_history_commit_count = full_status.len();
    drop(full_status);

    let mut window_status =
        load_status_commits(repo_root, Some(&since), follow_renames, fetch_limit)?;
    let window_status_cap_reached = window_status.len() as u64 > max_commits;
    window_status.truncate(max_commits as usize);
    let mut window_numstat = load_numstat_commits(repo_root, &since, follow_renames, fetch_limit)?;
    let window_numstat_cap_reached = window_numstat.len() as u64 > max_commits;
    window_numstat.truncate(max_commits as usize);
    let status_by_commit: BTreeMap<&str, &StatusCommit> = window_status
        .iter()
        .map(|commit| (commit.commit.as_str(), commit))
        .collect();
    let density = token_density(&tracked_paths, token_counts, line_counts);
    let mut aliases: BTreeMap<String, String> = tracked_paths
        .iter()
        .map(|path| (path.clone(), path.clone()))
        .collect();
    let mut accumulators: BTreeMap<String, FileAccumulator> = tracked_paths
        .iter()
        .map(|path| (path.clone(), FileAccumulator::default()))
        .collect();
    let mut commit_records = Vec::new();
    let mut baseline_commits = Vec::new();
    let mut bot_markers = pointer_strings(config, "/stewardship/bot_name_markers");
    if config.pointer("/stewardship/bot_name_markers").is_none() {
        bot_markers = vec!["bot".to_string(), "[bot]".to_string()];
    }
    let bot_markers: Vec<String> = bot_markers
        .into_iter()
        .map(|marker| marker.to_lowercase())
        .collect();
    let recent_cutoff =
        now.timestamp() - Duration::days(RECENT_MAINTAINER_WINDOW_DAYS).num_seconds();
    let late_window_seconds = Duration::days(LATE_CHURN_WINDOW_DAYS).num_seconds();

    for commit in window_numstat {
        let mut changes: BTreeMap<String, CommitFileChange> = BTreeMap::new();
        for entry in &commit.entries {
            let current_path = if follow_renames {
                map_numstat_with_lineage(entry, &aliases)
            } else {
                map_numstat_exact(entry, &tracked_paths)
            };
            let Some(current_path) = current_path else {
                continue;
            };
            let line_churn = entry.added.saturating_add(entry.deleted);
            let path_density = density.get(&current_path).copied().unwrap_or(1.0);
            let token_churn = (line_churn as f64 * path_density).round_ties_even() as usize;
            let aggregate = changes.entry(current_path).or_insert(CommitFileChange {
                added: 0,
                deleted: 0,
                line_churn: 0,
                token_churn: 0,
            });
            aggregate.added = aggregate.added.saturating_add(entry.added);
            aggregate.deleted = aggregate.deleted.saturating_add(entry.deleted);
            aggregate.line_churn = aggregate.line_churn.saturating_add(line_churn);
            aggregate.token_churn = aggregate.token_churn.saturating_add(token_churn);
        }

        if !changes.is_empty() {
            let elapsed_seconds = now.timestamp().saturating_sub(commit.timestamp);
            let age_days = if commit.timestamp == 0 {
                0.0
            } else {
                (elapsed_seconds.max(0) as f64) / 86_400.0
            };
            let recency_weight = 1.0 / (1.0 + age_days / RECENCY_HALF_WINDOW_DAYS);
            let is_late = commit.timestamp != 0 && elapsed_seconds <= late_window_seconds;
            let author = commit.author.clone();
            let author_is_bot = is_bot(&author, &bot_markers);

            for (path, change) in &changes {
                let accumulator = accumulators
                    .get_mut(path)
                    .expect("mapped history path must have an accumulator");
                accumulator.metrics.revisions_window += 1;
                accumulator.metrics.added_window = accumulator
                    .metrics
                    .added_window
                    .saturating_add(change.added);
                accumulator.metrics.deleted_window = accumulator
                    .metrics
                    .deleted_window
                    .saturating_add(change.deleted);
                accumulator.metrics.line_churn_window = accumulator
                    .metrics
                    .line_churn_window
                    .saturating_add(change.line_churn);
                accumulator.metrics.token_churn_window = accumulator
                    .metrics
                    .token_churn_window
                    .saturating_add(change.token_churn);
                accumulator.metrics.recency_weighted_commits += recency_weight;
                if is_late {
                    accumulator.late_token_churn += change.token_churn;
                }
                *accumulator.author_counts.entry(author.clone()).or_default() += 1;
                if commit.timestamp >= recent_cutoff {
                    accumulator.recent_authors.insert(author.clone());
                }
                if !author_is_bot && commit.timestamp > 0 {
                    accumulator.latest_non_bot_timestamp = Some(
                        accumulator
                            .latest_non_bot_timestamp
                            .unwrap_or(i64::MIN)
                            .max(commit.timestamp),
                    );
                }
            }

            let line_churn_by_path: BTreeMap<String, usize> = changes
                .iter()
                .map(|(path, change)| (path.clone(), change.line_churn))
                .collect();
            let paths: Vec<String> = line_churn_by_path.keys().cloned().collect();
            let status_commit = status_by_commit.get(commit.commit.as_str()).copied();
            let creation_changes = status_commit
                .map(|status| {
                    status
                        .changes
                        .iter()
                        .filter(|change| matches!(change, StatusChange::Path { status, .. } if status.starts_with('A')))
                        .count()
                })
                .unwrap_or_default();
            let subject = commit.subject.to_ascii_lowercase();
            let change_kind = if commit.parents.len() > 1 {
                "merge"
            } else if subject.contains("release")
                || subject.starts_with("bump version")
                || subject.starts_with("chore: version")
            {
                "release"
            } else if subject.contains("import")
                || subject.contains("vendor")
                || subject.contains("generated snapshot")
            {
                "import"
            } else if !paths.is_empty() && creation_changes >= paths.len() {
                "creation"
            } else {
                "maintenance"
            };
            let change_set_calibration = 1.0 / (paths.len().saturating_sub(1).max(1) as f64).sqrt();
            let calibration_weight = match change_kind {
                "merge" | "import" => 0.0,
                "release" => change_set_calibration * 0.1,
                _ => change_set_calibration,
            };
            let roots: BTreeSet<String> = paths.iter().map(|path| top_level_root(path)).collect();
            let line_weights: Vec<usize> =
                changes.values().map(|change| change.line_churn).collect();
            baseline_commits.push(BaselineCommit {
                file_count: paths.len(),
                total_token_delta: changes
                    .values()
                    .map(|change| change.token_churn as f64)
                    .sum(),
                top_level_root_count: roots.len(),
                change_entropy: round_to(shannon_entropy(line_weights), 6),
            });
            commit_records.push(CommitRecord {
                commit: commit.commit.clone(),
                timestamp: commit.timestamp,
                author,
                paths,
                line_churn_by_path,
                change_set_size: changes.len(),
                change_kind: change_kind.to_string(),
                calibration_weight: round_to(calibration_weight, 6),
            });
        }

        if follow_renames {
            apply_rename_aliases(
                &mut aliases,
                status_by_commit.get(commit.commit.as_str()).copied(),
            );
        }
    }

    for path in &tracked_paths {
        let accumulator = accumulators
            .get_mut(path)
            .expect("tracked path must have an accumulator");
        let first_seen_timestamp = first_seen.get(path).copied().flatten();
        accumulator.metrics.first_seen_timestamp = first_seen_timestamp;
        accumulator.metrics.age_days = first_seen_timestamp
            .map(|timestamp| {
                now.timestamp()
                    .saturating_sub(timestamp)
                    .max(0)
                    .div_euclid(86_400) as u64
            })
            .unwrap_or(0);
        accumulator.metrics.recency_weighted_commits =
            round_to(accumulator.metrics.recency_weighted_commits, 6);
        accumulator.metrics.relative_churn_window = round_to(
            accumulator.metrics.line_churn_window as f64
                / line_counts.get(path).copied().unwrap_or(0).max(1) as f64,
            6,
        );
        accumulator.metrics.late_churn_spike = round_to(
            accumulator.late_token_churn as f64
                / accumulator.metrics.token_churn_window.max(1) as f64,
            6,
        );
        accumulator.metrics.author_count_window = accumulator.author_counts.len();
        accumulator.metrics.author_entropy = round_to(
            shannon_entropy(accumulator.author_counts.values().copied()),
            6,
        );
        let total_author_commits: usize = accumulator.author_counts.values().sum();
        accumulator.metrics.top_author_share = if total_author_commits == 0 {
            0.0
        } else {
            round_to(
                accumulator
                    .author_counts
                    .values()
                    .copied()
                    .max()
                    .unwrap_or(0) as f64
                    / total_author_commits as f64,
                6,
            )
        };
        accumulator.metrics.days_since_non_bot_edit =
            accumulator.latest_non_bot_timestamp.map(|timestamp| {
                now.timestamp()
                    .saturating_sub(timestamp)
                    .max(0)
                    .div_euclid(86_400) as u64
            });
        accumulator.metrics.recent_maintainer_diversity = accumulator.recent_authors.len();
    }

    let metrics = accumulators
        .into_iter()
        .map(|(path, accumulator)| (path, accumulator.metrics))
        .collect();
    let mut baselines = repo_baselines(&baseline_commits);
    baselines["full_history_commit_count"] = json!(full_history_commit_count);
    baselines["window_status_commit_count"] = json!(window_status.len());
    baselines["window_numstat_commit_count"] = json!(commit_records.len());
    baselines["max_commits"] = json!(max_commits);
    baselines["history_cap_reached"] = json!(full_history_cap_reached);
    baselines["full_history_cap_status"] = json!(if full_history_cap_reached {
        "truncated"
    } else {
        "complete"
    });
    baselines["window_status_cap_status"] = json!(if window_status_cap_reached {
        "truncated"
    } else {
        "complete"
    });
    baselines["window_numstat_cap_status"] = json!(if window_numstat_cap_reached {
        "truncated"
    } else {
        "complete"
    });
    baselines["follow_renames"] = json!(follow_renames);
    Ok((metrics, commit_records, baselines))
}
