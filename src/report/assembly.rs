use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::support::{float_field, string_array, string_field, usize_field};
use crate::VERSION;
use crate::model::{Analysis, HealthRollup};

const REPORT_SCHEMA_VERSION: u64 = 4;
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

fn suppress_saturated_overlays(files: &mut [Value]) -> Vec<String> {
    let specs = [
        ("verification", "verification_gap"),
        ("navigation", "navigation_pressure"),
        ("blast_radius", "blast_radius_pressure"),
        ("stewardship", "stewardship_pressure"),
        ("semantic_drift", "semantic_drift_pressure"),
    ];
    let mut suppressed = Vec::new();
    for (family, pressure) in specs {
        let measured = files
            .iter()
            .filter_map(|file| {
                file.pointer(&format!("/overlays/{family}/{pressure}"))
                    .and_then(Value::as_f64)
            })
            .collect::<Vec<_>>();
        if measured.len() >= 10
            && measured.iter().filter(|value| **value >= 0.999).count() * 10 >= measured.len() * 9
        {
            suppressed.push(family.to_string());
            for file in files.iter_mut() {
                if let Some(overlays) = file.get_mut("overlays").and_then(Value::as_object_mut) {
                    overlays.remove(family);
                }
            }
        }
    }
    suppressed
}

fn repo_payload(analysis: &Analysis) -> Value {
    let mut repo = serde_json::to_value(&analysis.repo).unwrap_or_else(|_| json!({}));
    if let Some(object) = repo.as_object_mut() {
        object.remove("repo_root");
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
    let mut files = serialize_values(&analysis.files);
    let suppressed_saturated_overlays = suppress_saturated_overlays(&mut files);
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
    let config_bytes = serde_json::to_vec(&analysis.config).unwrap_or_default();
    let config_digest = hex::encode(Sha256::digest(config_bytes));
    let history_cap_reached = analysis
        .diagnostics
        .pointer("/history/history_cap_reached")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let history_complete = !analysis.repo.is_shallow && !history_cap_reached;
    json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "analyzer": {
            "name": "git-slop",
            "version": VERSION,
            "config_digest": config_digest,
            "context_tokenizer": analysis.config.pointer("/tokenization/context_tokenizer_name")
                .and_then(Value::as_str).unwrap_or("cl100k_base")
        },
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
        "scope": analysis.scope,
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
            "history_complete": history_complete
        },
        "evidence_completeness": {
            "history": if analysis.repo.is_shallow {
                "incomplete_shallow"
            } else if history_cap_reached {
                "incomplete_commit_cap"
            } else {
                "complete"
            },
            "repository_size": if files.len() < 10 { "low_support" } else { "sufficient" },
            "history_window_days": analysis.config.pointer("/history/churn_window_days").cloned().unwrap_or(Value::Null),
            "history_max_commits": analysis.config.pointer("/history/max_commits").cloned().unwrap_or(Value::Null),
            "first_seen_age": if history_complete { "complete" } else { "bounded" },
            "churn_window": if analysis.repo.is_shallow { "incomplete_shallow" } else { "complete_window" },
            "author_evidence": if analysis.repo.is_shallow { "incomplete_shallow" } else { "complete_window" },
            "relationship_evidence": if history_complete { "complete" } else { "bounded" },
            "missing_test_evidence_count": overlays.pointer("/verification/files")
                .and_then(Value::as_array)
                .map(|records| records.iter().filter(|record| record.get("verification_gap").and_then(Value::as_f64).unwrap_or_default() >= 0.8).count())
                .unwrap_or_default(),
            "relationship_support": if analysis.organization.relationships.pointer("/temporal_coupling_edges").and_then(Value::as_array).is_some_and(Vec::is_empty) { "low_support" } else { "available" }
        },
        "diagnostics": {
            "suppressed_saturated_overlays": suppressed_saturated_overlays,
            "relationship_count": analysis.organization.relationships.as_object().map(|collections| collections.values().filter_map(Value::as_array).map(Vec::len).sum::<usize>()).unwrap_or_default(),
            "structural_tokens_omitted": true,
            "analysis": analysis.diagnostics
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
        "collection_metadata": {
            "files": {"total": files.len(), "returned": files.len(), "limit": null, "truncated": false},
            "folders": {"total": folders.len(), "returned": folders.len(), "limit": null, "truncated": false},
            "action_queue": {"total": action_queue.len(), "returned": action_queue.len(), "limit": null, "truncated": false},
            "health.findings": {"total": health.findings.len(), "returned": health.findings.len(), "limit": null, "truncated": false},
            "health.refactor_candidates": {"total": health.refactor_candidates.len(), "returned": health.refactor_candidates.len(), "limit": null, "truncated": false},
            "health.watchlist": {"total": health.watchlist.len(), "returned": health.watchlist.len(), "limit": null, "truncated": false}
        },
        "organization_metrics": analysis.organization.organization_metrics,
        "relationships": analysis.organization.relationships,
        "clusters": analysis.organization.clusters
    })
}
