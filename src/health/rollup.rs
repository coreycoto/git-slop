use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::model::*;
use crate::model::{Analysis, Finding, HealthRollup, parent_folders};

pub(super) fn distribution(values: &[usize]) -> Value {
    if values.is_empty() {
        return json!({
            "count": 0,
            "total": 0,
            "p50": 0.0,
            "p90": 0.0,
            "p95": 0.0,
            "p99": 0.0,
            "max": 0,
            "top_1_share": 0.0,
            "top_5_share": 0.0,
            "top_10_share": 0.0
        });
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let total = sorted.iter().sum::<usize>();
    let mut descending = sorted.clone();
    descending.reverse();
    let share = |count: usize| -> f64 {
        if total == 0 {
            0.0
        } else {
            descending.iter().take(count).sum::<usize>() as f64 / total as f64
        }
    };
    json!({
        "count": sorted.len(),
        "total": total,
        "p50": percentile(&sorted, 0.50),
        "p90": percentile(&sorted, 0.90),
        "p95": percentile(&sorted, 0.95),
        "p99": percentile(&sorted, 0.99),
        "max": sorted.last().copied().unwrap_or_default(),
        "top_1_share": share(1),
        "top_5_share": share(5),
        "top_10_share": share(10)
    })
}

fn percentile(sorted: &[usize], percentile: f64) -> f64 {
    match sorted.len() {
        0 => 0.0,
        1 => sorted[0] as f64,
        length => {
            let rank = (length - 1) as f64 * percentile;
            let lower = rank.floor() as usize;
            let upper = rank.ceil() as usize;
            if lower == upper {
                sorted[lower] as f64
            } else {
                let weight = rank - lower as f64;
                sorted[lower] as f64 * (1.0 - weight) + sorted[upper] as f64 * weight
            }
        }
    }
}

fn candidate_sort(left: &Value, right: &Value) -> Ordering {
    band_rank(string_field(right, "band"))
        .cmp(&band_rank(string_field(left, "band")))
        .then_with(|| usize_field(right, "tokens").cmp(&usize_field(left, "tokens")))
        .then_with(|| string_field(left, "path").cmp(string_field(right, "path")))
}

fn file_candidate(file: &Value, band: &str, parent_tokens: usize) -> Value {
    json!({
        "kind": "file",
        "path": string_field(file, "path"),
        "profile": file_profile(file),
        "class": classification(file),
        "tokens": usize_field(file, "tokens"),
        "parent_tokens": parent_tokens,
        "band": band,
        "slop_band": string_field(file, "slop_band"),
        "slop_score": float_field(file, "slop_score"),
        "reason_codes": string_array(file.get("reason_codes"))
    })
}

fn folder_candidate(
    folder: &Value,
    band: &str,
    direct_files: usize,
    recursive_files: usize,
    direct_tokens: usize,
    recursive_tokens: usize,
    parent_tokens: usize,
) -> Value {
    json!({
        "kind": "folder",
        "path": string_field(folder, "path"),
        "profile": "agent_context",
        "class": classification(folder),
        "files": direct_files,
        "descendant_files": recursive_files,
        "tokens": direct_tokens,
        "recursive_tokens": recursive_tokens,
        "parent_tokens": parent_tokens,
        "band": band,
        "slop_band": string_field(folder, "slop_band"),
        "slop_score": float_field(folder, "slop_score"),
        "reason_codes": string_array(folder.get("reason_codes"))
    })
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

pub fn humanize_reason_code(reason: &str) -> String {
    match reason {
        "critical_token_cost" => "exceeds the configured context budget".to_string(),
        "high_token_cost" => "is near the configured context budget".to_string(),
        "high_revision_frequency" => "changes frequently".to_string(),
        "high_relative_churn" => "has high churn relative to its size".to_string(),
        other => other.replace('_', " "),
    }
}

pub(super) fn finding_for_file(file: &Value, config: &Value) -> Option<Finding> {
    if matches!(
        string_field(file, "classification"),
        "generated" | "vendored" | "snapshot" | "fixture" | "migration_fixture"
    ) {
        return None;
    }
    let context_band = string_field(file, "context_band");
    let slop_band = string_field(file, "slop_band");
    let tokens = usize_field(file, "tokens");
    let healthy_max = config_u64(
        config,
        "/tokenization/context_bands/healthy_max_tokens",
        DEFAULT_HEALTHY_MAX,
    ) as usize;
    let is_watchlist = context_band == "healthy" && tokens >= healthy_max.saturating_mul(3) / 4;
    if !matches!(
        context_band,
        "warning" | "critical" | "refactor_required" | "budget_exceeded"
    ) && !matches!(slop_band, "high" | "critical")
        && !is_watchlist
    {
        return None;
    }
    let severity = if matches!(
        context_band,
        "critical" | "refactor_required" | "budget_exceeded"
    ) || slop_band == "critical"
    {
        "error"
    } else if context_band == "warning" || slop_band == "high" {
        "warning"
    } else {
        "notice"
    };
    let raw_reasons = string_array(file.get("reason_codes"));
    let mut reasons: Vec<String> = raw_reasons
        .iter()
        .map(|reason| humanize_reason_code(reason))
        .collect();
    if reasons.is_empty() {
        reasons.push(match context_band {
            "critical" | "refactor_required" | "budget_exceeded" => {
                format!("{tokens} tokens exceed the configured fail threshold")
            }
            "warning" => format!("{tokens} tokens are in the configured warning band"),
            _ => format!("{tokens} tokens leave limited context headroom"),
        });
    }
    let path = string_field(file, "path").to_string();
    let title = if matches!(
        context_band,
        "critical" | "refactor_required" | "budget_exceeded"
    ) {
        "Context budget exceeded"
    } else if context_band == "warning" {
        "Context budget warning"
    } else if slop_band == "high" || slop_band == "critical" {
        "High maintenance pressure"
    } else {
        "Context headroom is narrowing"
    };
    Some(Finding {
        path: path.clone(),
        profile: file_profile(file).to_string(),
        severity: severity.to_string(),
        title: title.to_string(),
        message: format!(
            "{} has {} tokens and a slop score of {:.1}: {}.",
            path,
            tokens,
            float_field(file, "slop_score"),
            reasons.join("; ")
        ),
        next_command: format!("git-slop explain --path {}", shell_quote(&path)),
        slop_band: slop_band.to_string(),
        context_band: context_band.to_string(),
        slop_score: float_field(file, "slop_score"),
        tokens,
        reasons,
    })
}

pub(super) fn build_health_rollup_from_values(
    files: &[Value],
    folders: &[Value],
    config: &Value,
) -> HealthRollup {
    let mut file_band_counts = BTreeMap::from([
        ("compact".to_string(), 0),
        ("healthy".to_string(), 0),
        ("warning".to_string(), 0),
        ("budget_exceeded".to_string(), 0),
    ]);
    let mut folder_band_counts = file_band_counts.clone();
    let mut profile_totals: BTreeMap<String, Totals> = BTreeMap::new();
    let mut language_totals: BTreeMap<(String, String), Totals> = BTreeMap::new();
    let mut file_tokens = Vec::new();
    let mut folder_tokens = Vec::new();
    let mut refactor_candidates = Vec::new();
    let mut watchlist = Vec::new();
    let healthy_max = config_u64(
        config,
        "/tokenization/context_bands/healthy_max_tokens",
        DEFAULT_HEALTHY_MAX,
    ) as usize;
    let folder_healthy_max = config_u64(
        config,
        "/health/folder_bands/healthy_max_direct_tokens",
        DEFAULT_FOLDER_HEALTHY_MAX,
    ) as usize;
    let folder_warning_files = config_u64(
        config,
        "/health/folder_bands/warning_max_direct_files",
        DEFAULT_FOLDER_WARNING_FILES,
    ) as usize;
    let mut agent_folder_paths = BTreeSet::new();
    let mut agent_direct_folder_totals: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut agent_recursive_folder_totals: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for file in files {
        let path = string_field(file, "path");
        let tokens = usize_field(file, "tokens");
        let direct = direct_parent(path);
        let direct_totals = agent_direct_folder_totals.entry(direct).or_default();
        direct_totals.0 += 1;
        direct_totals.1 += tokens;
        for parent in parent_folders(path) {
            agent_folder_paths.insert(parent.clone());
            let recursive_totals = agent_recursive_folder_totals.entry(parent).or_default();
            recursive_totals.0 += 1;
            recursive_totals.1 += tokens;
        }
    }

    for file in files {
        let profile = file_profile(file).to_string();
        profile_totals
            .entry(profile.clone())
            .or_default()
            .add_file(file);
        let language = file
            .get("language")
            .and_then(Value::as_str)
            .filter(|language| !language.is_empty())
            .unwrap_or("Unknown")
            .to_string();
        language_totals
            .entry((profile.clone(), language))
            .or_default()
            .add_file(file);
        let tokens = usize_field(file, "tokens");
        file_tokens.push(tokens);
        let band = health_file_band(file);
        *file_band_counts.entry(band.clone()).or_default() += 1;
        let parent_tokens = agent_recursive_folder_totals
            .get(&direct_parent(string_field(file, "path")))
            .map(|totals| totals.1)
            .unwrap_or_default();
        if matches!(band.as_str(), "warning" | "budget_exceeded") {
            refactor_candidates.push(file_candidate(file, &band, parent_tokens));
        } else if band == "healthy" && tokens >= healthy_max.saturating_mul(3) / 4 {
            watchlist.push(file_candidate(file, &band, parent_tokens));
        }
    }

    for folder in folders {
        let raw_path = string_field(folder, "path").trim_matches('/');
        let path = if raw_path.is_empty() { "." } else { raw_path };
        if !agent_folder_paths.contains(path) {
            continue;
        }
        let (direct_files, direct_tokens) = agent_direct_folder_totals
            .get(path)
            .copied()
            .unwrap_or_default();
        let (recursive_files, recursive_tokens) = agent_recursive_folder_totals
            .get(path)
            .copied()
            .unwrap_or_default();
        folder_tokens.push(direct_tokens);
        let band = folder_health_band_for(direct_tokens as u64, direct_files as u64, config);
        *folder_band_counts.entry(band.clone()).or_default() += 1;
        let parent_tokens = if path == "." {
            0
        } else {
            agent_recursive_folder_totals
                .get(&direct_parent(path))
                .map(|totals| totals.1)
                .unwrap_or_default()
        };
        if matches!(band.as_str(), "warning" | "budget_exceeded") {
            refactor_candidates.push(folder_candidate(
                folder,
                &band,
                direct_files,
                recursive_files,
                direct_tokens,
                recursive_tokens,
                parent_tokens,
            ));
        } else if band == "healthy"
            && (direct_tokens >= folder_healthy_max.saturating_mul(3) / 4
                || direct_files >= folder_warning_files.saturating_mul(3) / 4)
        {
            watchlist.push(folder_candidate(
                folder,
                &band,
                direct_files,
                recursive_files,
                direct_tokens,
                recursive_tokens,
                parent_tokens,
            ));
        }
    }

    refactor_candidates.sort_by(candidate_sort);
    watchlist.sort_by(candidate_sort);

    let profile_rollups = profile_totals
        .iter()
        .map(|(name, totals)| {
            json!({
                "name": name,
                "totals": {
                    "files": totals.files,
                    "lines": totals.lines,
                    "code": totals.code,
                    "comments": totals.comments,
                    "blanks": totals.blanks,
                    "tokens": totals.tokens
                }
            })
        })
        .collect();
    let mut language_rollups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for ((profile, language), totals) in language_totals {
        let profile_tokens = profile_totals
            .get(&profile)
            .map(|item| item.tokens)
            .unwrap_or_default();
        language_rollups
            .entry(profile.clone())
            .or_default()
            .push(json!({
                "profile": profile,
                "language": language,
                "files": totals.files,
                "lines": totals.lines,
                "code": totals.code,
                "comments": totals.comments,
                "blanks": totals.blanks,
                "tokens": totals.tokens,
                "token_share": if profile_tokens == 0 {
                    0.0
                } else {
                    totals.tokens as f64 / profile_tokens as f64
                }
            }));
    }
    for values in language_rollups.values_mut() {
        values.sort_by(|left, right| {
            usize_field(right, "tokens")
                .cmp(&usize_field(left, "tokens"))
                .then_with(|| string_field(left, "language").cmp(string_field(right, "language")))
        });
    }

    let mut ranked_files = files.iter().collect::<Vec<_>>();
    ranked_files.sort_by(|left, right| {
        float_field(right, "slop_score")
            .partial_cmp(&float_field(left, "slop_score"))
            .unwrap_or(Ordering::Equal)
            .then_with(|| usize_field(right, "tokens").cmp(&usize_field(left, "tokens")))
            .then_with(|| string_field(left, "path").cmp(string_field(right, "path")))
    });
    let findings = ranked_files
        .into_iter()
        .filter_map(|file| finding_for_file(file, config))
        .collect();

    HealthRollup {
        file_band_counts,
        folder_band_counts,
        profile_rollups,
        language_rollups,
        file_distribution: distribution(&file_tokens),
        folder_distribution: distribution(&folder_tokens),
        refactor_candidates,
        watchlist,
        findings,
    }
}

pub fn build_health_rollup(analysis: &Analysis) -> HealthRollup {
    let files = analysis
        .files
        .iter()
        .filter_map(|file| serde_json::to_value(file).ok())
        .collect::<Vec<_>>();
    let folders = analysis
        .folders
        .iter()
        .filter_map(|folder| serde_json::to_value(folder).ok())
        .collect::<Vec<_>>();
    build_health_rollup_from_values(&files, &folders, &analysis.config)
}

pub fn health_rollup_from_report(report: &Value) -> Result<HealthRollup> {
    if !matches!(
        report.get("schema_version").and_then(Value::as_u64),
        Some(4 | 5)
    ) {
        bail!("repository-health rendering requires report schema 4 or 5.");
    }
    Ok(report
        .get("health")
        .cloned()
        .and_then(|value| serde_json::from_value::<HealthRollup>(value).ok())
        .unwrap_or_else(|| {
            let files = report
                .get("files")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let folders = report
                .get("folders")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let config = report.get("config").unwrap_or(&Value::Null);
            build_health_rollup_from_values(files, folders, config)
        }))
}
