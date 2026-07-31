use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::config::{pointer_f64, pointer_u64};
use crate::model::{FileAnalysis, FolderAnalysis, parent_folders};

const DEFAULT_COMPACT_MAX_TOKENS: u64 = 3_072;
const DEFAULT_HEALTHY_MAX_TOKENS: u64 = 8_000;
const DEFAULT_WARNING_MAX_TOKENS: u64 = 10_000;
const DEFAULT_AGE_HALF_LIFE_DAYS: f64 = 180.0;
const DEFAULT_CONTEXT_WEIGHT: f64 = 0.60;
const DEFAULT_AGE_WEIGHT: f64 = 0.20;
const DEFAULT_CHURN_WEIGHT: f64 = 0.20;

fn round_to(value: f64, digits: i32) -> f64 {
    let factor = 10_f64.powi(digits);
    (value * factor).round_ties_even() / factor
}

fn p95(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut sorted: Vec<f64> = values.into_iter().collect();
    if sorted.is_empty() {
        return 1.0;
    }
    sorted.sort_by(f64::total_cmp);
    let rank = ((sorted.len() as f64 * 0.95).ceil() as usize).max(1);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

pub fn context_band_for_tokens(tokens: usize, config: &Value) -> String {
    let compact_max = pointer_u64(
        config,
        "/tokenization/context_bands/compact_max_tokens",
        DEFAULT_COMPACT_MAX_TOKENS,
    ) as usize;
    let healthy_max = pointer_u64(
        config,
        "/tokenization/context_bands/healthy_max_tokens",
        DEFAULT_HEALTHY_MAX_TOKENS,
    ) as usize;
    let warning_max = pointer_u64(
        config,
        "/tokenization/context_bands/warning_max_tokens",
        DEFAULT_WARNING_MAX_TOKENS,
    ) as usize;
    if tokens <= compact_max {
        "compact"
    } else if tokens <= healthy_max {
        "healthy"
    } else if tokens <= warning_max {
        "warning"
    } else {
        "critical"
    }
    .to_string()
}

pub fn context_pressure_for_tokens(tokens: usize, config: &Value) -> f64 {
    let warning_max = pointer_u64(
        config,
        "/tokenization/context_bands/warning_max_tokens",
        DEFAULT_WARNING_MAX_TOKENS,
    )
    .max(1) as f64;
    (tokens as f64 / warning_max).min(1.0)
}

pub fn slop_band_for_score(score: f64) -> String {
    if score >= 85.0 {
        "critical"
    } else if score >= 65.0 {
        "high"
    } else if score >= 50.0 {
        "moderate"
    } else {
        "low"
    }
    .to_string()
}

fn age_pressure(age_days: u64, config: &Value) -> f64 {
    if age_days == 0 {
        return 0.0;
    }
    let half_life = pointer_f64(
        config,
        "/history/age_half_life_days",
        DEFAULT_AGE_HALF_LIFE_DAYS,
    );
    if half_life <= 0.0 {
        return 1.0;
    }
    1.0 - 2_f64.powf(-(age_days as f64 / half_life))
}

fn reason_codes(record: &FileAnalysis) -> Vec<String> {
    let mut reasons = Vec::new();
    match record.context_band.as_str() {
        "critical" => reasons.push("critical_token_cost".to_string()),
        "warning" => reasons.push("high_token_cost".to_string()),
        _ => {}
    }
    if record.age_days >= 180 {
        reasons.push("old_file".to_string());
    }
    if record.revision_norm >= 0.8 {
        reasons.push("high_revision_frequency".to_string());
    }
    if record.relative_churn_norm >= 0.8 {
        reasons.push("high_relative_churn".to_string());
    }
    if record.age_days >= 180 && record.churn_pressure >= 0.6 {
        reasons.push("old_and_volatile".to_string());
    }
    reasons
}

/// Apply the stable 60/20/20 hotspot model in place.
///
/// Repository-relative churn components use Python's nearest-rank p95 ordering.
/// Intermediate metrics are rounded to six decimals and the final score to one
/// decimal before banding, matching the public Python report contract.
pub fn apply_scoring(records: &mut [FileAnalysis], config: &Value) {
    let revision_p95 = p95(records.iter().map(|record| record.revisions_window as f64)).max(1.0);
    let relative_churn_p95 = p95(records.iter().map(|record| record.relative_churn_window));
    let relative_churn_denom = if relative_churn_p95 > 0.0 {
        relative_churn_p95
    } else {
        1.0
    };
    let context_weight = pointer_f64(config, "/scoring/context_weight", DEFAULT_CONTEXT_WEIGHT);
    let age_weight = pointer_f64(config, "/scoring/age_weight", DEFAULT_AGE_WEIGHT);
    let churn_weight = pointer_f64(config, "/scoring/churn_weight", DEFAULT_CHURN_WEIGHT);

    for record in records {
        let raw_age_pressure = age_pressure(record.age_days, config);
        let raw_revision_norm = (record.revisions_window as f64 / revision_p95).min(1.0);
        let raw_relative_churn_norm =
            (record.relative_churn_window / relative_churn_denom).min(1.0);
        let raw_churn_pressure = 0.6 * raw_revision_norm + 0.4 * raw_relative_churn_norm;
        let raw_score = 100.0
            * (context_weight * record.context_pressure
                + age_weight * raw_age_pressure
                + churn_weight * raw_churn_pressure);

        record.age_pressure = round_to(raw_age_pressure, 6);
        record.revision_norm = round_to(raw_revision_norm, 6);
        record.relative_churn_norm = round_to(raw_relative_churn_norm, 6);
        record.churn_pressure = round_to(raw_churn_pressure, 6);
        record.slop_score = round_to(raw_score, 1);
        record.slop_band = slop_band_for_score(record.slop_score);
        record.reason_codes = reason_codes(record);
    }
}

pub fn folder_health_band(
    direct_tokens: usize,
    direct_file_count: usize,
    config: &Value,
) -> String {
    let compact_max_tokens = pointer_u64(
        config,
        "/health/folder_bands/compact_max_direct_tokens",
        31_999,
    ) as usize;
    let healthy_max_tokens = pointer_u64(
        config,
        "/health/folder_bands/healthy_max_direct_tokens",
        128_000,
    ) as usize;
    let warning_max_tokens = pointer_u64(
        config,
        "/health/folder_bands/warning_max_direct_tokens",
        256_000,
    ) as usize;
    let warning_max_files =
        pointer_u64(config, "/health/folder_bands/warning_max_direct_files", 17) as usize;
    let refactor_max_files = pointer_u64(
        config,
        "/health/folder_bands/refactor_required_max_direct_files",
        37,
    ) as usize;

    if direct_tokens > warning_max_tokens || direct_file_count > refactor_max_files {
        "refactor_required"
    } else if direct_tokens > healthy_max_tokens || direct_file_count > warning_max_files {
        "warning"
    } else if direct_tokens > compact_max_tokens {
        "healthy"
    } else {
        "compact"
    }
    .to_string()
}

fn direct_parent(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .filter(|parent| !parent.is_empty())
        .unwrap_or(".")
}

fn folder_classification(descendants: &[&FileAnalysis]) -> String {
    let classifications: BTreeSet<&str> = descendants
        .iter()
        .map(|record| record.classification.as_str())
        .collect();
    if classifications.len() == 1 {
        classifications
            .into_iter()
            .next()
            .unwrap_or("other")
            .to_string()
    } else {
        "mixed".to_string()
    }
}

fn mean(values: impl IntoIterator<Item = f64>) -> f64 {
    let values: Vec<f64> = values.into_iter().collect();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn file_cost(record: &FileAnalysis, section: &str, key: &str) -> f64 {
    record
        .costs
        .get(section)
        .and_then(|value| value.get(key))
        .and_then(Value::as_f64)
        .unwrap_or_default()
}

fn folder_costs(descendants: &[&FileAnalysis], total_tokens: usize, config: &Value) -> Value {
    let mut token_sizes: Vec<usize> = descendants.iter().map(|record| record.tokens).collect();
    token_sizes.sort_unstable_by(|left, right| right.cmp(left));
    let top_tokens = token_sizes.first().copied().unwrap_or(0);
    json!({
        "load": {
            "file_token_count": top_tokens,
            "folder_token_count": total_tokens,
            "top_file_share": round_to(top_tokens as f64 / total_tokens.max(1) as f64, 6),
            "top_3_file_share": round_to(
                token_sizes.iter().take(3).sum::<usize>() as f64 / total_tokens.max(1) as f64,
                6,
            ),
            "token_concentration_ratio": round_to(
                top_tokens as f64 / total_tokens.max(1) as f64,
                6,
            ),
            "context_band": context_band_for_tokens(total_tokens, config),
            "load_pressure": round_to(context_pressure_for_tokens(total_tokens, config), 6),
        },
        "volatility": {
            "commit_count_window": descendants
                .iter()
                .map(|record| record.revisions_window as f64)
                .sum::<f64>(),
            "recency_weighted_commits": round_to(
                descendants.iter().map(|record| record.recency_weighted_commits).sum(),
                6,
            ),
            "line_churn_window": round_to(
                descendants
                    .iter()
                    .map(|record| record.line_churn_window as f64)
                    .sum(),
                6,
            ),
            "token_churn_window": descendants
                .iter()
                .map(|record| record.token_churn_window)
                .sum::<usize>(),
            "relative_token_churn": round_to(
                mean(descendants.iter().map(|record| {
                    record.token_churn_window as f64 / record.tokens.max(1) as f64
                })),
                6,
            ),
            "late_churn_spike": round_to(
                mean(descendants.iter().map(|record| record.late_churn_spike)),
                6,
            ),
            "volatility_pressure": round_to(
                mean(descendants.iter().map(|record| record.churn_pressure)),
                6,
            ),
        },
        "coordination": {
            "files_touched_per_change": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "files_touched_per_change")
                })),
                6,
            ),
            "folders_touched_per_change": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "folders_touched_per_change")
                })),
                6,
            ),
            "edit_hunks_per_change": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "edit_hunks_per_change")
                })),
                6,
            ),
            "cochange_degree": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "cochange_degree")
                })),
                6,
            ),
            "cochange_centrality": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "cochange_centrality")
                })),
                6,
            ),
            "cross_folder_cochange_ratio": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "cross_folder_cochange_ratio")
                })),
                6,
            ),
            "change_diffusion": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "change_diffusion")
                })),
                6,
            ),
            "coordination_pressure": round_to(
                mean(descendants.iter().map(|record| {
                    file_cost(record, "coordination", "coordination_pressure")
                })),
                6,
            ),
        },
    })
}

fn aggregate_overlay_value(
    folder_path: &str,
    overlay_name: &str,
    descendants: &[&FileAnalysis],
) -> Value {
    let overlay_values: Vec<&Value> = descendants
        .iter()
        .filter_map(|record| record.overlays.get(overlay_name))
        .filter(|value| !value.is_null())
        .collect();
    if overlay_values.is_empty() {
        return Value::Null;
    }
    let overlay_count = overlay_values.len();
    let mut numeric: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut booleans: BTreeMap<String, bool> = BTreeMap::new();
    let mut lists: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for value in overlay_values {
        let Some(object) = value.as_object() else {
            continue;
        };
        for (key, value) in object {
            if key == "path" {
                continue;
            }
            if let Some(boolean) = value.as_bool() {
                booleans
                    .entry(key.clone())
                    .and_modify(|current| *current |= boolean)
                    .or_insert(boolean);
            } else if let Some(number) = value.as_f64() {
                numeric.entry(key.clone()).or_default().push(number);
            } else if let Some(items) = value.as_array() {
                let values = lists.entry(key.clone()).or_default();
                values.extend(
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned),
                );
            }
        }
    }
    let mut result = serde_json::Map::new();
    result.insert("path".to_string(), json!(folder_path));
    result.insert("overlay_name".to_string(), json!(overlay_name));
    for (key, values) in numeric {
        result.insert(key, json!(round_to(mean(values), 6)));
    }
    for (key, value) in booleans {
        result.insert(key, json!(value));
    }
    for (key, values) in lists {
        result.insert(
            key,
            Value::Array(values.into_iter().take(20).map(Value::String).collect()),
        );
    }
    result.insert("descendant_file_count".to_string(), json!(overlay_count));
    Value::Object(result)
}

fn folder_overlays(folder_path: &str, descendants: &[&FileAnalysis]) -> Value {
    let overlay_names = [
        "organization_health",
        "verification",
        "navigation",
        "blast_radius",
        "stewardship",
        "semantic_drift",
    ];
    let mut result = serde_json::Map::new();
    for overlay_name in overlay_names {
        result.insert(
            overlay_name.to_string(),
            aggregate_overlay_value(folder_path, overlay_name, descendants),
        );
    }
    Value::Object(result)
}

pub fn build_folder_analysis(
    path: &str,
    descendants: &[&FileAnalysis],
    config: &Value,
) -> Option<FolderAnalysis> {
    if descendants.is_empty() {
        return None;
    }
    let mut ranked = descendants.to_vec();
    ranked.sort_by(|left, right| {
        right
            .slop_score
            .total_cmp(&left.slop_score)
            .then_with(|| right.tokens.cmp(&left.tokens))
            .then_with(|| left.path.cmp(&right.path))
    });
    let top = ranked[0];
    let total_tokens = descendants.iter().map(|record| record.tokens).sum();
    let direct: Vec<&FileAnalysis> = descendants
        .iter()
        .copied()
        .filter(|record| direct_parent(&record.path) == path)
        .collect();
    let direct_tokens = direct.iter().map(|record| record.tokens).sum();
    let mut seen_reasons = BTreeSet::new();
    let mut reason_codes = Vec::new();
    for record in &ranked {
        for reason in &record.reason_codes {
            if seen_reasons.insert(reason.clone()) {
                reason_codes.push(reason.clone());
            }
        }
    }
    Some(FolderAnalysis {
        path: path.to_string(),
        descendant_file_count: descendants.len(),
        direct_file_count: direct.len(),
        bytes: descendants.iter().map(|record| record.bytes).sum(),
        lines: descendants.iter().map(|record| record.lines).sum(),
        tokens: total_tokens,
        direct_tokens,
        context_band: context_band_for_tokens(total_tokens, config),
        health_band: folder_health_band(direct_tokens, direct.len(), config),
        context_pressure: round_to(context_pressure_for_tokens(total_tokens, config), 6),
        slop_score: top.slop_score,
        slop_band: top.slop_band.clone(),
        reason_codes,
        top_file_path: top.path.clone(),
        classification: folder_classification(descendants),
        costs: folder_costs(descendants, total_tokens, config),
        overlays: folder_overlays(path, descendants),
    })
}

pub fn build_folder_analyses(files: &[FileAnalysis], config: &Value) -> Vec<FolderAnalysis> {
    let mut grouped: BTreeMap<String, Vec<&FileAnalysis>> = BTreeMap::new();
    for record in files {
        for folder in parent_folders(&record.path) {
            grouped.entry(folder).or_default().push(record);
        }
    }
    let mut folders: Vec<FolderAnalysis> = grouped
        .into_iter()
        .filter_map(|(path, descendants)| build_folder_analysis(&path, &descendants, config))
        .collect();
    folders.sort_by(|left, right| {
        (left.path != ".")
            .cmp(&(right.path != "."))
            .then_with(|| left.path.cmp(&right.path))
    });
    folders
}

pub fn aggregate_folders(files: &[FileAnalysis], config: &Value) -> Vec<FolderAnalysis> {
    build_folder_analyses(files, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, tokens: usize, revisions: usize, relative_churn: f64) -> FileAnalysis {
        FileAnalysis {
            path: path.to_string(),
            bytes: tokens * 4,
            lines: tokens / 2,
            blank_lines: 0,
            code_lines: tokens / 2,
            comment_lines: 0,
            language: "Rust".to_string(),
            profile: "agent_context".to_string(),
            classification: "source".to_string(),
            tokens,
            context_band: "compact".to_string(),
            context_pressure: 0.0,
            structural_tokens: Vec::new(),
            structural_token_count: 0,
            top_structural_terms: Vec::new(),
            age_days: 0,
            revisions_window: revisions,
            recency_weighted_commits: 0.0,
            added_window: 0,
            deleted_window: 0,
            churn_lines_window: 0,
            line_churn_window: 0,
            token_churn_window: 0,
            relative_churn_window: relative_churn,
            late_churn_spike: 0.0,
            author_count_window: 0,
            author_entropy: 0.0,
            top_author_share: 0.0,
            days_since_non_bot_edit: None,
            recent_maintainer_diversity: 0,
            age_pressure: 0.0,
            revision_norm: 0.0,
            relative_churn_norm: 0.0,
            churn_pressure: 0.0,
            slop_score: 0.0,
            slop_band: String::new(),
            reason_codes: Vec::new(),
            costs: json!({}),
            overlays: json!({}),
        }
    }

    #[test]
    fn context_and_slop_thresholds_match_public_contract() {
        let config = json!({});
        assert_eq!(context_band_for_tokens(3_072, &config), "compact");
        assert_eq!(context_band_for_tokens(3_073, &config), "healthy");
        assert_eq!(context_band_for_tokens(8_001, &config), "warning");
        assert_eq!(context_band_for_tokens(10_001, &config), "critical");
        assert_eq!(slop_band_for_score(49.9), "low");
        assert_eq!(slop_band_for_score(50.0), "moderate");
        assert_eq!(slop_band_for_score(65.0), "high");
        assert_eq!(slop_band_for_score(85.0), "critical");
    }

    #[test]
    fn scoring_uses_nearest_rank_p95_and_caps_outliers() {
        let mut files: Vec<FileAnalysis> = (1..=20)
            .map(|revision| file(&format!("src/{revision}.rs"), 100, revision, 0.0))
            .collect();
        apply_scoring(&mut files, &json!({}));
        assert_eq!(files[17].revision_norm, round_to(18.0 / 19.0, 6));
        assert_eq!(files[18].revision_norm, 1.0);
        assert_eq!(files[19].revision_norm, 1.0);
        assert_eq!(files[19].slop_score, 12.0);
    }

    #[test]
    fn scoring_preserves_reason_order_and_rounded_band() {
        let mut record = file("src/legacy.rs", 12_000, 10, 2.0);
        record.context_band = "critical".to_string();
        record.context_pressure = 1.0;
        record.age_days = 180;
        apply_scoring(std::slice::from_mut(&mut record), &json!({}));
        assert_eq!(record.slop_score, 90.0);
        assert_eq!(record.slop_band, "critical");
        assert_eq!(
            record.reason_codes,
            vec![
                "critical_token_cost",
                "old_file",
                "high_revision_frequency",
                "high_relative_churn",
                "old_and_volatile",
            ]
        );
    }

    #[test]
    fn stable_relative_line_churn_drives_score_and_action_reason() {
        let mut files = vec![
            file("src/quiet.rs", 100, 1, 0.1),
            file("src/volatile.rs", 100, 1, 2.0),
        ];
        apply_scoring(&mut files, &json!({}));

        assert_eq!(files[0].relative_churn_norm, 0.05);
        assert_eq!(files[1].relative_churn_norm, 1.0);
        assert!(files[1].slop_score > files[0].slop_score);
        assert!(
            !files[0]
                .reason_codes
                .contains(&"high_relative_churn".to_string())
        );
        assert!(
            files[1]
                .reason_codes
                .contains(&"high_relative_churn".to_string())
        );
    }

    #[test]
    fn stable_history_fields_keep_schema_four_integer_shapes() {
        let mut record = file("src/history.rs", 100, 2, 0.5);
        record.added_window = 7;
        record.deleted_window = 3;
        record.churn_lines_window = 10;
        record.line_churn_window = 10;
        record.token_churn_window = 24;

        let value = serde_json::to_value(record).unwrap();
        assert_eq!(value["added_window"].as_u64(), Some(7));
        assert_eq!(value["deleted_window"].as_u64(), Some(3));
        assert_eq!(value["churn_lines_window"].as_u64(), Some(10));
        assert_eq!(value["token_churn_window"].as_u64(), Some(24));
        assert_eq!(value["relative_churn_window"].as_f64(), Some(0.5));
    }

    #[test]
    fn folder_aggregation_tracks_descendant_and_direct_pressure_separately() {
        let mut top = file("src/top.rs", 20_000, 1, 0.0);
        top.slop_score = 60.0;
        top.slop_band = "moderate".to_string();
        top.reason_codes = vec!["high_token_cost".to_string()];
        let mut nested = file("src/nested/child.rs", 20_000, 1, 0.0);
        nested.slop_score = 70.0;
        nested.slop_band = "high".to_string();
        nested.reason_codes = vec!["high_relative_churn".to_string()];
        let folders = build_folder_analyses(&[top, nested], &json!({}));
        let root = folders.iter().find(|folder| folder.path == ".").unwrap();
        let src = folders.iter().find(|folder| folder.path == "src").unwrap();
        let nested = folders
            .iter()
            .find(|folder| folder.path == "src/nested")
            .unwrap();
        assert_eq!(root.direct_file_count, 0);
        assert_eq!(root.descendant_file_count, 2);
        assert_eq!(src.direct_file_count, 1);
        assert_eq!(src.direct_tokens, 20_000);
        assert_eq!(src.tokens, 40_000);
        assert_eq!(src.top_file_path, "src/nested/child.rs");
        assert_eq!(src.reason_codes, ["high_relative_churn", "high_token_cost"]);
        assert_eq!(nested.direct_file_count, 1);
    }

    #[test]
    fn folder_health_uses_direct_token_and_file_limits() {
        let config = json!({});
        assert_eq!(folder_health_band(31_999, 17, &config), "compact");
        assert_eq!(folder_health_band(32_000, 17, &config), "healthy");
        assert_eq!(folder_health_band(128_001, 17, &config), "warning");
        assert_eq!(folder_health_band(1, 18, &config), "warning");
        assert_eq!(folder_health_band(256_001, 1, &config), "refactor_required");
        assert_eq!(folder_health_band(1, 38, &config), "refactor_required");
    }
}
