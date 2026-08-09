use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::config::{pointer_bool, pointer_strings, pointer_u64};
use crate::model::{CommitRecord, HistoryMetrics, top_level_root};

const DEFAULT_HISTORY_WINDOW_DAYS: u64 = 180;
const RECENCY_HALF_WINDOW_DAYS: f64 = 30.0;
const LATE_CHURN_WINDOW_DAYS: i64 = 30;
const RECENT_MAINTAINER_WINDOW_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StatusChange {
    Path { status: String, path: String },
    Rename { old_path: String, new_path: String },
    Copy { old_path: String, new_path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusCommit {
    commit: String,
    timestamp: i64,
    author: String,
    parents: Vec<String>,
    subject: String,
    changes: Vec<StatusChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumstatEntry {
    added: usize,
    deleted: usize,
    paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumstatCommit {
    commit: String,
    timestamp: i64,
    author: String,
    parents: Vec<String>,
    subject: String,
    entries: Vec<NumstatEntry>,
}

#[derive(Debug, Clone)]
struct CommitFileChange {
    added: usize,
    deleted: usize,
    line_churn: usize,
    token_churn: usize,
}

#[derive(Debug, Default)]
struct FileAccumulator {
    metrics: HistoryMetrics,
    author_counts: BTreeMap<String, usize>,
    recent_authors: BTreeSet<String>,
    latest_non_bot_timestamp: Option<i64>,
    late_token_churn: usize,
}

#[derive(Debug)]
struct BaselineCommit {
    file_count: usize,
    total_token_delta: f64,
    top_level_root_count: usize,
    change_entropy: f64,
}

fn normalized_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn author_key(name: &str, email: &str) -> String {
    let name = name.trim();
    let email = email.trim();
    format!("{name} <{email}>")
}

fn parse_status_log(raw: &str) -> Vec<StatusCommit> {
    let tokens: Vec<&str> = raw.split('\0').collect();
    let mut commits = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] != "commit" || index + 4 >= tokens.len() {
            index += 1;
            continue;
        }
        let commit = tokens[index + 1].trim().to_string();
        let timestamp = tokens[index + 2].trim().parse::<i64>().unwrap_or(0);
        let author = author_key(tokens[index + 3], tokens[index + 4]);
        let extended = tokens.get(index + 5).is_some_and(|value| {
            value.is_empty()
                || value.split_whitespace().all(|parent| {
                    parent.len() == 40
                        && parent
                            .chars()
                            .all(|character| character.is_ascii_hexdigit())
                })
        });
        let parents = if extended {
            tokens[index + 5]
                .split_whitespace()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let subject = if extended {
            tokens.get(index + 6).unwrap_or(&"").to_string()
        } else {
            String::new()
        };
        index += if extended { 7 } else { 5 };
        let mut changes = Vec::new();
        while index < tokens.len() && tokens[index] != "commit" {
            let status = tokens[index].trim_start_matches('\n');
            index += 1;
            if status.is_empty() {
                continue;
            }
            match status.as_bytes()[0] {
                b'R' | b'C' => {
                    if index + 1 >= tokens.len() {
                        break;
                    }
                    let old_path = normalized_path(tokens[index]);
                    let new_path = normalized_path(tokens[index + 1]);
                    index += 2;
                    if old_path.is_empty() || new_path.is_empty() {
                        continue;
                    }
                    if status.starts_with('R') {
                        changes.push(StatusChange::Rename { old_path, new_path });
                    } else {
                        changes.push(StatusChange::Copy { old_path, new_path });
                    }
                }
                _ => {
                    if index >= tokens.len() {
                        break;
                    }
                    let path = normalized_path(tokens[index]);
                    index += 1;
                    if !path.is_empty() {
                        changes.push(StatusChange::Path {
                            status: status.to_string(),
                            path,
                        });
                    }
                }
            }
        }
        commits.push(StatusCommit {
            commit,
            timestamp,
            author,
            parents,
            subject,
            changes,
        });
    }
    commits
}

fn parse_numstat_log(raw: &str) -> Vec<NumstatCommit> {
    let tokens: Vec<&str> = raw.split('\0').collect();
    let mut commits = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] != "commit" || index + 4 >= tokens.len() {
            index += 1;
            continue;
        }
        let commit = tokens[index + 1].trim().to_string();
        let timestamp = tokens[index + 2].trim().parse::<i64>().unwrap_or(0);
        let author = author_key(tokens[index + 3], tokens[index + 4]);
        let extended = tokens.get(index + 5).is_some_and(|value| {
            value.is_empty()
                || value.split_whitespace().all(|parent| {
                    parent.len() == 40
                        && parent
                            .chars()
                            .all(|character| character.is_ascii_hexdigit())
                })
        });
        let parents = if extended {
            tokens[index + 5]
                .split_whitespace()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let subject = if extended {
            tokens.get(index + 6).unwrap_or(&"").to_string()
        } else {
            String::new()
        };
        index += if extended { 7 } else { 5 };
        let mut entries = Vec::new();
        while index < tokens.len() && tokens[index] != "commit" {
            let stat_line = tokens[index].trim_start_matches('\n');
            index += 1;
            if stat_line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = stat_line.splitn(3, '\t').collect();
            if parts.len() < 2 {
                continue;
            }
            let mut paths = Vec::new();
            if parts.get(2).is_some_and(|path| !path.is_empty()) {
                paths.push(normalized_path(parts[2]));
            } else {
                if index >= tokens.len() || tokens[index] == "commit" {
                    break;
                }
                if !tokens[index].is_empty() {
                    paths.push(normalized_path(tokens[index]));
                }
                index += 1;
                if index < tokens.len() && tokens[index] != "commit" && !tokens[index].is_empty() {
                    paths.push(normalized_path(tokens[index]));
                    index += 1;
                }
            }
            if parts[0] == "-" || parts[1] == "-" {
                continue;
            }
            let (Ok(added), Ok(deleted)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>())
            else {
                continue;
            };
            entries.push(NumstatEntry {
                added,
                deleted,
                paths,
            });
        }
        commits.push(NumstatCommit {
            commit,
            timestamp,
            author,
            parents,
            subject,
            entries,
        });
    }
    commits
}

fn git_has_head(repo_root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .context("failed to execute git")?;
    Ok(output.status.success())
}

fn stream_git_log<T>(
    repo_root: &Path,
    args: &[String],
    parse: fn(&str) -> Vec<T>,
) -> Result<Vec<T>> {
    let mut child = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to execute git")?;
    let stdout = child
        .stdout
        .take()
        .context("git log stdout was unavailable")?;
    let mut reader = std::io::BufReader::new(stdout);
    let mut token = Vec::new();
    let mut commit = String::new();
    let mut records = Vec::new();
    loop {
        token.clear();
        let bytes = reader.read_until(0, &mut token)?;
        if bytes == 0 {
            break;
        }
        if token.last() == Some(&0) {
            token.pop();
        }
        let value = String::from_utf8_lossy(&token);
        if value.trim_start_matches('\n') == "commit" && !commit.is_empty() {
            records.extend(parse(&commit));
            commit.clear();
        }
        commit.push_str(&value);
        commit.push('\0');
    }
    if !commit.is_empty() {
        records.extend(parse(&commit));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "git {} failed{}",
            args.join(" "),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(records)
}

fn load_status_commits(
    repo_root: &Path,
    since: Option<&str>,
    follow_renames: bool,
    max_commits: u64,
) -> Result<Vec<StatusCommit>> {
    let mut args = vec![
        "log".to_string(),
        "--no-show-signature".to_string(),
        "--name-status".to_string(),
        "-z".to_string(),
        "--format=commit%x00%H%x00%ct%x00%an%x00%ae%x00%P%x00%s".to_string(),
        if follow_renames {
            "--find-renames".to_string()
        } else {
            "--no-renames".to_string()
        },
        format!("--max-count={max_commits}"),
    ];
    if let Some(since) = since {
        args.push(format!("--since={since}"));
    }
    stream_git_log(repo_root, &args, parse_status_log)
}

fn load_numstat_commits(
    repo_root: &Path,
    since: &str,
    follow_renames: bool,
    max_commits: u64,
) -> Result<Vec<NumstatCommit>> {
    let args = vec![
        "log".to_string(),
        "--no-show-signature".to_string(),
        "--numstat".to_string(),
        "-z".to_string(),
        "--format=commit%x00%H%x00%ct%x00%an%x00%ae%x00%P%x00%s".to_string(),
        format!("--since={since}"),
        format!("--max-count={max_commits}"),
        if follow_renames {
            "--find-renames".to_string()
        } else {
            "--no-renames".to_string()
        },
    ];
    // Preserve the established ordering contract: commits are emitted newest
    // first and rename aliases are expanded while walking back.
    stream_git_log(repo_root, &args, parse_numstat_log)
}

fn apply_rename_aliases(aliases: &mut BTreeMap<String, String>, commit: Option<&StatusCommit>) {
    let Some(commit) = commit else {
        return;
    };
    for change in &commit.changes {
        let StatusChange::Rename { old_path, new_path } = change else {
            continue;
        };
        if let Some(current_path) = aliases.get(new_path).cloned() {
            aliases.insert(old_path.clone(), current_path);
        }
    }
}

fn mapped_paths_for_status_change(
    change: &StatusChange,
    aliases: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    match change {
        StatusChange::Rename { old_path, new_path } => {
            if let Some(current) = aliases.get(new_path).or_else(|| aliases.get(old_path)) {
                result.insert(current.clone());
            }
        }
        StatusChange::Copy { new_path, .. } => {
            if let Some(current) = aliases.get(new_path) {
                result.insert(current.clone());
            }
        }
        StatusChange::Path { path, .. } => {
            if let Some(current) = aliases.get(path) {
                result.insert(current.clone());
            }
        }
    }
    result
}

fn first_seen_exact(
    tracked_paths: &BTreeSet<String>,
    commits: &[StatusCommit],
) -> BTreeMap<String, Option<i64>> {
    let mut appearances = BTreeMap::new();
    let mut fallbacks = BTreeMap::new();
    for commit in commits {
        for change in &commit.changes {
            match change {
                StatusChange::Rename { new_path, .. } if tracked_paths.contains(new_path) => {
                    appearances.insert(new_path.clone(), commit.timestamp);
                    fallbacks.insert(new_path.clone(), commit.timestamp);
                }
                StatusChange::Path { status, path } if tracked_paths.contains(path) => {
                    fallbacks.insert(path.clone(), commit.timestamp);
                    if status.starts_with('A') {
                        appearances.insert(path.clone(), commit.timestamp);
                    }
                }
                _ => {}
            }
        }
    }
    tracked_paths
        .iter()
        .map(|path| {
            (
                path.clone(),
                appearances
                    .get(path)
                    .copied()
                    .or_else(|| fallbacks.get(path).copied()),
            )
        })
        .collect()
}

fn first_seen_with_lineage(
    tracked_paths: &BTreeSet<String>,
    commits: &[StatusCommit],
) -> BTreeMap<String, Option<i64>> {
    let mut aliases: BTreeMap<String, String> = tracked_paths
        .iter()
        .map(|path| (path.clone(), path.clone()))
        .collect();
    let mut result: BTreeMap<String, Option<i64>> = tracked_paths
        .iter()
        .map(|path| (path.clone(), None))
        .collect();
    for commit in commits {
        let mut touched = BTreeSet::new();
        for change in &commit.changes {
            touched.extend(mapped_paths_for_status_change(change, &aliases));
        }
        for path in touched {
            result.insert(path, Some(commit.timestamp));
        }
        apply_rename_aliases(&mut aliases, Some(commit));
    }
    result
}

fn map_numstat_exact(entry: &NumstatEntry, tracked_paths: &BTreeSet<String>) -> Option<String> {
    match entry.paths.as_slice() {
        [old_path, new_path, ..] => {
            if tracked_paths.contains(new_path) {
                Some(new_path.clone())
            } else if tracked_paths.contains(old_path) {
                Some(old_path.clone())
            } else {
                None
            }
        }
        [path] if tracked_paths.contains(path) => Some(path.clone()),
        _ => None,
    }
}

fn map_numstat_with_lineage(
    entry: &NumstatEntry,
    aliases: &BTreeMap<String, String>,
) -> Option<String> {
    match entry.paths.as_slice() {
        [old_path, new_path, ..] => aliases
            .get(new_path)
            .or_else(|| aliases.get(old_path))
            .cloned(),
        [path] => aliases.get(path).cloned(),
        _ => None,
    }
}

fn token_density(
    paths: &BTreeSet<String>,
    token_counts: &BTreeMap<String, usize>,
    line_counts: &BTreeMap<String, usize>,
) -> BTreeMap<String, f64> {
    paths
        .iter()
        .map(|path| {
            let lines = line_counts.get(path).copied().unwrap_or(0).max(1);
            let tokens = token_counts.get(path).copied().unwrap_or(0);
            (path.clone(), ((tokens as f64) / (lines as f64)).max(1.0))
        })
        .collect()
}

fn shannon_entropy(weights: impl IntoIterator<Item = usize>) -> f64 {
    let weights: Vec<usize> = weights.into_iter().collect();
    let total: usize = weights.iter().sum();
    if total == 0 {
        return 0.0;
    }
    weights
        .into_iter()
        .filter(|weight| *weight > 0)
        .map(|weight| {
            let probability = weight as f64 / total as f64;
            -probability * probability.log2()
        })
        .sum()
}

fn round_to(value: f64, digits: i32) -> f64 {
    let factor = 10_f64.powi(digits);
    (value * factor).round_ties_even() / factor
}

fn nearest_rank_percentile(values: &[f64], quantile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((sorted.len() as f64 * quantile).ceil() as usize).max(1);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn repo_baselines(commits: &[BaselineCommit]) -> Value {
    if commits.is_empty() {
        return json!({
            "p95_files_touched": 0.0,
            "p99_files_touched": 0.0,
            "p95_token_delta_mass": 0.0,
            "p95_top_level_root_spread": 0.0,
            "p95_change_entropy": 0.0,
        });
    }
    let files_touched: Vec<f64> = commits
        .iter()
        .map(|commit| commit.file_count as f64)
        .collect();
    let token_delta: Vec<f64> = commits
        .iter()
        .map(|commit| commit.total_token_delta)
        .collect();
    let root_spread: Vec<f64> = commits
        .iter()
        .map(|commit| commit.top_level_root_count as f64)
        .collect();
    let entropy: Vec<f64> = commits.iter().map(|commit| commit.change_entropy).collect();
    json!({
        "p95_files_touched": nearest_rank_percentile(&files_touched, 0.95),
        "p99_files_touched": nearest_rank_percentile(&files_touched, 0.99),
        "p95_token_delta_mass": nearest_rank_percentile(&token_delta, 0.95),
        "p95_top_level_root_spread": nearest_rank_percentile(&root_spread, 0.95),
        "p95_change_entropy": nearest_rank_percentile(&entropy, 0.95),
    })
}

fn is_bot(author: &str, markers: &[String]) -> bool {
    let author = author.to_lowercase();
    markers.iter().any(|marker| author.contains(marker))
}

fn empty_result(
    analyzed_paths: &[String],
    status: &str,
) -> (BTreeMap<String, HistoryMetrics>, Vec<CommitRecord>, Value) {
    let metrics = analyzed_paths
        .iter()
        .map(|path| (normalized_path(path), HistoryMetrics::default()))
        .collect();
    let mut diagnostics = repo_baselines(&[]);
    diagnostics["history_status"] = json!(status);
    diagnostics["history_cap_reached"] = json!(false);
    (metrics, Vec::new(), diagnostics)
}

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
