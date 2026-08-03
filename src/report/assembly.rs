use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::support::{float_field, string_array, string_field, usize_field};
use crate::model::{Analysis, HealthRollup};

const REPORT_SCHEMA_VERSION: u64 = 4;
const MAX_SUMMARY_LIMIT: usize = 25;
const ORGANIZATION_OVERLAY: &str = "organization_health";
const ADDITIVE_OVERLAYS: [&str; 5] = [
    "verification",
    "navigation",
    "blast_radius",
    "stewardship",
    "semantic_drift",
];

fn serialize_values<T: serde::Serialize>(items: &[T]) -> Vec<Value> {
    items
        .iter()
        .filter_map(|item| serde_json::to_value(item).ok())
        .collect()
}

fn repo_payload(analysis: &Analysis) -> Value {
    let mut repo = serde_json::to_value(&analysis.repo).unwrap_or_else(|_| json!({}));
    if let Some(object) = repo.as_object_mut() {
        object.insert(
            "head_sha".to_string(),
            analysis
                .repo
                .head_commit
                .as_ref()
                .map_or(Value::Null, |value| json!(value)),
        );
        object.insert(
            "remote_url".to_string(),
            analysis
                .repo
                .git_remote_url
                .as_ref()
                .map_or(Value::Null, |value| json!(value)),
        );
        object.insert(
            "has_head_commit".to_string(),
            json!(analysis.repo.head_commit.is_some()),
        );
    }
    repo
}

fn action_queue_from_files(files: &[Value]) -> Vec<Value> {
    let mut ranked = files.to_vec();
    ranked.sort_by(|left, right| {
        float_field(right, "slop_score")
            .partial_cmp(&float_field(left, "slop_score"))
            .unwrap_or(Ordering::Equal)
            .then_with(|| usize_field(right, "tokens").cmp(&usize_field(left, "tokens")))
            .then_with(|| string_field(left, "path").cmp(string_field(right, "path")))
    });
    ranked
        .into_iter()
        .take(MAX_SUMMARY_LIMIT)
        .map(|file| {
            let reasons = string_array(file.get("reason_codes"));
            let pure_context = !reasons.is_empty()
                && reasons.iter().all(|reason| {
                    matches!(reason.as_str(), "high_token_cost" | "critical_token_cost")
                });
            json!({
                "path": string_field(&file, "path"),
                "slop_score": float_field(&file, "slop_score"),
                "slop_band": string_field(&file, "slop_band"),
                "context_band": string_field(&file, "context_band"),
                "tokens": usize_field(&file, "tokens"),
                "age_days": usize_field(&file, "age_days"),
                "revisions_window": usize_field(&file, "revisions_window"),
                "churn_pressure": float_field(&file, "churn_pressure"),
                "reason_codes": reasons,
                "is_pure_context_hotspot": pure_context
            })
        })
        .collect()
}

fn overlay_with_path(path: &str, value: &Value) -> Value {
    let mut payload = value.clone();
    match payload.as_object_mut() {
        Some(object) => {
            object.insert("path".to_string(), json!(path));
            payload
        }
        None => json!({"path": path}),
    }
}

fn named_overlay_entries(records: &[Value], overlay_name: &str) -> Vec<Value> {
    records
        .iter()
        .filter_map(|record| {
            let path = record.get("path")?.as_str()?;
            let overlay = record.get("overlays")?.get(overlay_name)?;
            (!overlay.is_null()).then(|| overlay_with_path(path, overlay))
        })
        .collect()
}

fn map_overlay_entries(values: &BTreeMap<String, Value>) -> Vec<Value> {
    values
        .iter()
        .map(|(path, value)| overlay_with_path(path, value))
        .collect()
}

fn organization_overlay(analysis: &Analysis) -> Value {
    let metrics = &analysis.organization.organization_metrics;
    let analysis_status = metrics
        .get("analysis_status")
        .and_then(Value::as_str)
        .unwrap_or("experimental");
    let analysis_version = metrics
        .get("analysis_version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    json!({
        "enabled": true,
        "experimental": true,
        "analysis_status": analysis_status,
        "analysis_version": analysis_version,
        "repo_baselines": metrics.get("repo_baselines").cloned().unwrap_or_else(|| json!({})),
        "files": map_overlay_entries(&analysis.organization.file_overlays),
        "folders": map_overlay_entries(&analysis.organization.folder_overlays),
        "relationships": analysis.organization.relationships,
        "clusters": analysis.organization.clusters,
        "findings": {
            "top_structural_files": analysis.organization.top_structural_files
        }
    })
}

fn canonical_overlays(analysis: &Analysis, files: &[Value], folders: &[Value]) -> Value {
    let mut overlays = Map::new();
    overlays.insert(
        ORGANIZATION_OVERLAY.to_string(),
        organization_overlay(analysis),
    );
    for overlay_name in ADDITIVE_OVERLAYS {
        let mut wrapper = json!({
            "enabled": true,
            "experimental": true,
            "analysis_status": "experimental",
            "analysis_version": 2,
            "files": named_overlay_entries(files, overlay_name),
            "folders": named_overlay_entries(folders, overlay_name)
        });
        if overlay_name == "semantic_drift" {
            wrapper
                .as_object_mut()
                .expect("overlay wrapper is an object")
                .insert("findings".to_string(), json!([]));
        }
        overlays.insert(overlay_name.to_string(), wrapper);
    }
    Value::Object(overlays)
}

fn verification_paths(overlays: &Value) -> Vec<String> {
    let mut records = overlays
        .pointer("/verification/files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    records.sort_by(|left, right| {
        float_field(right, "verification_gap")
            .partial_cmp(&float_field(left, "verification_gap"))
            .unwrap_or(Ordering::Equal)
            .then_with(|| string_field(left, "path").cmp(string_field(right, "path")))
    });
    records
        .iter()
        .filter_map(|value| value.get("path").and_then(Value::as_str))
        .take(5)
        .map(ToOwned::to_owned)
        .collect()
}

fn top_structural_paths(analysis: &Analysis) -> Vec<String> {
    analysis
        .organization
        .top_structural_files
        .iter()
        .filter_map(|value| value.get("path").and_then(Value::as_str))
        .take(5)
        .map(ToOwned::to_owned)
        .collect()
}

pub fn assemble_report(analysis: &Analysis, health: &HealthRollup) -> Value {
    let files = serialize_values(&analysis.files);
    let folders = serialize_values(&analysis.folders);
    let action_queue = if analysis.action_queue.is_empty() {
        action_queue_from_files(&files)
    } else {
        analysis.action_queue.clone()
    };
    let overlays = canonical_overlays(analysis, &files, &folders);
    let critical_context_file_count = files
        .iter()
        .filter(|file| {
            matches!(
                string_field(file, "context_band"),
                "critical" | "refactor_required"
            )
        })
        .count();
    let critical_slop_file_count = files
        .iter()
        .filter(|file| string_field(file, "slop_band") == "critical")
        .count();
    let top_hotspots = action_queue
        .iter()
        .filter_map(|item| item.get("path").and_then(Value::as_str))
        .take(5)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let health_value = serde_json::to_value(health).unwrap_or_else(|_| json!({}));
    json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "generated_at": analysis.generated_at,
        "analyzed_revision_at": analysis.analyzed_revision_at
            .as_ref()
            .or(analysis.repo.head_commit_timestamp.as_ref()),
        "summary": {
            "top_hotspots": top_hotspots,
            "top_structural_files": top_structural_paths(analysis),
            "top_verification_gaps": verification_paths(&overlays),
            "health": {
                "file_band_counts": health.file_band_counts,
                "folder_band_counts": health.folder_band_counts
            }
        },
        "repo": repo_payload(analysis),
        "config": analysis.config,
        "stats": {
            "tracked_file_count": analysis.tracked_file_count,
            "analyzed_file_count": files.len(),
            "skipped_ignored_count": analysis.skipped.ignored,
            "skipped_missing_count": analysis.skipped.missing,
            "skipped_binary_count": analysis.skipped.binary,
            "skipped_undecodable_count": analysis.skipped.undecodable,
            "critical_context_file_count": critical_context_file_count,
            "critical_slop_file_count": critical_slop_file_count,
            "history_complete": !analysis.repo.is_shallow
        },
        "files": files,
        "folders": folders,
        "action_queue": action_queue,
        "costs": {
            "load": {"analysis_status": "stable", "analysis_version": 1},
            "volatility": {"analysis_status": "stable", "analysis_version": 1},
            "coordination": {"analysis_status": "stable", "analysis_version": 1}
        },
        "overlays": overlays,
        "health": health_value,
        "organization_metrics": analysis.organization.organization_metrics,
        "relationships": analysis.organization.relationships,
        "clusters": analysis.organization.clusters
    })
}
