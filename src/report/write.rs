use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};

use super::assembly::assemble_report;
use super::render::{render_compatibility_summary, render_terminal};
use crate::config;
use crate::health::render_health_from_report;
use crate::model::{
    Analysis, FileAnalysis, FindResult, FolderAnalysis, HealthRollup, ScopeIdentity,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn cap_collection(report: &mut Value, key: &str, limit: usize) {
    let Some(records) = report.get_mut(key).and_then(Value::as_array_mut) else {
        return;
    };
    let total = records.len();
    records.truncate(limit);
    report["collection_metadata"][key] = json!({
        "total": total,
        "returned": records.len(),
        "limit": limit,
        "truncated": total > records.len()
    });
}

fn collect_prioritized_paths(report: &Value) -> Vec<String> {
    fn push_path(path: Option<&str>, seen: &mut BTreeSet<String>, paths: &mut Vec<String>) {
        if let Some(path) = path.filter(|path| !path.is_empty()) {
            if seen.insert(path.to_string()) {
                paths.push(path.to_string());
            }
        }
    }

    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for pointer in [
        "/health/findings",
        "/health/refactor_candidates",
        "/health/watchlist",
        "/action_queue",
        "/ranked_files",
    ] {
        for record in report
            .pointer(pointer)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            push_path(
                record.get("path").and_then(Value::as_str),
                &mut seen,
                &mut paths,
            );
        }
    }
    if let Some(relationships) = report
        .pointer("/overlays/organization_health/relationships")
        .and_then(Value::as_object)
    {
        for records in relationships.values().filter_map(Value::as_array) {
            for record in records {
                push_path(
                    record.get("source_path").and_then(Value::as_str),
                    &mut seen,
                    &mut paths,
                );
                push_path(
                    record.get("target_path").and_then(Value::as_str),
                    &mut seen,
                    &mut paths,
                );
            }
        }
    }
    paths
}

fn compact_files(report: &mut Value, limit: usize) -> BTreeSet<String> {
    let priorities = collect_prioritized_paths(report);
    let records = report
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .expect("canonical reports contain files");
    let total = records.len();
    let original = std::mem::take(records);
    let mut by_path = original
        .iter()
        .filter_map(|record| Some((record.get("path")?.as_str()?.to_string(), record.clone())))
        .collect::<BTreeMap<_, _>>();
    for path in priorities {
        if records.len() >= limit {
            break;
        }
        if let Some(record) = by_path.remove(&path) {
            records.push(record);
        }
    }
    for record in original {
        if records.len() >= limit {
            break;
        }
        let Some(path) = record.get("path").and_then(Value::as_str) else {
            continue;
        };
        if by_path.remove(path).is_some() {
            records.push(record);
        }
    }
    let retained = records
        .iter()
        .filter_map(|record| record.get("path").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    report["collection_metadata"]["files"] = json!({
        "total": total,
        "returned": records.len(),
        "limit": limit,
        "truncated": total > records.len()
    });
    retained
}

fn retain_path_collection(
    report: &mut Value,
    pointer: &str,
    metadata_key: &str,
    retained: &BTreeSet<String>,
    limit: Option<usize>,
) {
    let Some(records) = report.pointer_mut(pointer).and_then(Value::as_array_mut) else {
        return;
    };
    let total = records.len();
    records.retain(|record| {
        record
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| retained.contains(path))
    });
    if let Some(limit) = limit {
        records.truncate(limit);
    }
    let returned = records.len();
    report["collection_metadata"][metadata_key] = json!({
        "total": total,
        "returned": returned,
        "limit": limit,
        "truncated": total > returned
    });
}

fn retain_relationship_references(report: &mut Value, retained: &BTreeSet<String>) {
    let Some(relationships) = report
        .pointer_mut("/overlays/organization_health/relationships")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for records in relationships.values_mut().filter_map(Value::as_array_mut) {
        records.retain(|record| {
            ["source_path", "target_path"].into_iter().all(|key| {
                record
                    .get(key)
                    .and_then(Value::as_str)
                    .is_none_or(|path| retained.contains(path))
            })
        });
    }
}

fn retain_cluster_references(report: &mut Value, retained: &BTreeSet<String>) {
    let Some(clusters) = report
        .pointer_mut("/overlays/organization_health/clusters")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for records in clusters.values_mut().filter_map(Value::as_array_mut) {
        records.retain(|record| {
            record
                .get("member_paths")
                .and_then(Value::as_array)
                .is_none_or(|members| {
                    members
                        .iter()
                        .filter_map(Value::as_str)
                        .all(|path| retained.contains(path))
                })
        });
    }
}

fn apply_report_profile(report: &mut Value, profile: &str) {
    report["diagnostics"]["report_profile"] = json!(profile);
    report["diagnostics"]["report_profile_semantics"] = json!(match profile {
        "compact" =>
            "bounded presentation with exhaustive comparison index and resolvable retained references",
        "standard" =>
            "complete primary records with bounded high-cardinality relationship evidence",
        "full_evidence" => "complete primary records and unbounded retained evidence",
        _ => "unknown report profile",
    });
    if profile == "full_evidence" {
        return;
    }
    if profile == "standard" {
        for pointer in [
            "/overlays/organization_health/relationships/duplicate_neighborhoods",
            "/overlays/organization_health/relationships/near_duplicate_neighborhoods",
            "/overlays/organization_health/relationships/temporal_coupling_edges",
            "/overlays/organization_health/relationships/lexical_affinity_edges",
            "/overlays/organization_health/relationships/boundary_leakage_edges",
        ] {
            if let Some(records) = report.pointer_mut(pointer).and_then(Value::as_array_mut) {
                records.truncate(2_000);
            }
        }
        return;
    }
    let retained = compact_files(report, 250);
    for (pointer, metadata_key, limit) in [
        ("/health/findings", "health.findings", None),
        (
            "/health/refactor_candidates",
            "health.refactor_candidates",
            None,
        ),
        ("/health/watchlist", "health.watchlist", None),
        ("/action_queue", "action_queue", Some(100)),
        ("/ranked_files", "ranked_files", Some(250)),
    ] {
        retain_path_collection(report, pointer, metadata_key, &retained, limit);
    }
    retain_relationship_references(report, &retained);
    retain_cluster_references(report, &retained);
    cap_collection(report, "folders", 250);
    report["diagnostics"]["compact_profile_note"] = json!(
        "Collections are deterministically bounded; use --report-profile full-evidence for complete records."
    );
}

fn compressed_bytes(format: &str, source: &[u8]) -> Result<Option<(String, Vec<u8>)>> {
    match format {
        "none" => Ok(None),
        "gzip" => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(source)?;
            Ok(Some(("report.json.gz".to_string(), encoder.finish()?)))
        }
        "zstd" => Ok(Some((
            "report.json.zst".to_string(),
            zstd::stream::encode_all(source, 3)?,
        ))),
        value => anyhow::bail!("unsupported report compression {value:?}"),
    }
}

pub fn write_json_atomically(path: &Path, value: &Value) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let temporary = temporary_directory(parent, "report-migration");
    fs::write(&temporary, serde_json::to_string_pretty(value)? + "\n")
        .with_context(|| format!("failed to write {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("failed to publish {}", path.display()))
}

pub fn schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/coreycoto/git-slop/blob/v0.11.2/schemas/report-5.json",
        "title": "Git Slop report schema 5",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "analyzer", "generated_at", "analyzed_revision_at", "repo", "scope", "config", "stats", "summary", "files", "folders", "ranked_files", "action_queue", "costs", "overlays", "health", "diagnostics", "collection_metadata", "evidence_completeness", "terminology"],
        "properties": {
            "schema_version": {"const": 5},
            "generated_at": {"type": "string", "format": "date-time"},
            "analyzed_revision_at": {"type": ["string", "null"], "format": "date-time"},
            "analyzer": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name", "version", "report_profile", "analysis_contract_version", "config_digest", "analysis_config_digest", "evidence_config_digest", "policy_config_digest", "presentation_config_digest", "context_tokenizer"],
                "properties": {
                    "name": {"const": "git-slop"},
                    "version": {"type": "string"},
                    "report_profile": {"enum": ["compact", "standard", "full_evidence"]},
                    "analysis_clock": {"type": "string", "format": "date-time"},
                    "analysis_contract_version": {"type": "integer", "minimum": 1},
                    "config_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                    "analysis_config_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                    "evidence_config_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                    "policy_config_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                    "presentation_config_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                    "context_tokenizer": {"type": "string"}
                }
            },
            "repo": {"type": "object", "additionalProperties": false, "required": ["repo_name", "repository_id", "repository_identity_source", "branch", "head_commit_timestamp", "is_shallow", "detached_head", "worktree_clean", "staged_change_count", "modified_tracked_file_count", "untracked_file_count", "worktree_state_digest", "analyzed_content_digest", "head_sha", "remote_url", "has_head_commit"], "properties": {"repo_name":{"type":"string"},"repository_id":{"type":["string","null"]},"repository_identity_source":{"type":["string","null"]},"branch":{"type":["string","null"]},"head_commit_timestamp":{"type":["string","null"],"format":"date-time"},"is_shallow":{"type":"boolean"},"detached_head":{"type":"boolean"},"worktree_clean":{"type":"boolean"},"staged_change_count":{"type":"integer"},"modified_tracked_file_count":{"type":"integer"},"untracked_file_count":{"type":"integer"},"worktree_state_digest":{"type":"string","pattern":"^[a-f0-9]{64}$"},"analyzed_content_digest":{"type":["string","null"]},"head_sha":{"type":["string","null"]},"remote_url":{"type":["string","null"]},"has_head_commit":{"type":"boolean"}}},
            "scope": {
                "type": "object",
                "additionalProperties": false,
                "required": ["mode", "path", "selected_path_count", "selected_path_digest"],
                "properties": {
                    "mode": {"enum": ["repository", "scoped"]},
                    "path": {"type": ["string", "null"]},
                    "selected_path_count": {"type": "integer", "minimum": 0},
                    "selected_path_digest": {"type": "string", "pattern": "^[a-f0-9]{64}$"}
                }
            },
            "config": {"type": "object"},
            "stats": {"type": "object"},
            "summary": {"type": "object"},
            "files": {"type": "array", "items": {"$ref": "#/$defs/file"}},
            "folders": {"type": "array", "items": {"$ref": "#/$defs/folder"}},
            "compare_index": {
                "type": "object",
                "additionalProperties": false,
                "required": ["files", "folders"],
                "properties": {
                    "files": {"type": "array", "items": {"$ref": "#/$defs/compare_record"}},
                    "folders": {"type": "array", "items": {"$ref": "#/$defs/compare_record"}}
                }
            },
            "action_queue": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["path", "profile", "slop_score", "slop_band", "context_band", "tokens", "age_days", "revisions_window", "churn_pressure", "reason_codes", "is_pure_context_hotspot", "severity", "evidence_status", "next_action"], "properties":{"path":{"type":"string"},"profile":{"enum":["agent_context","data_context"]},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"tokens":{"type":"integer"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"churn_pressure":{"type":"number"},"reason_codes":{"type":"array","items":{"type":"string"}},"is_pure_context_hotspot":{"type":"boolean"},"severity":{"enum":["error","warning","notice"]},"evidence_status":{"type":"string"},"next_action":{"type":"string"}}}},
            "ranked_files": {"type": "array", "items": {"$ref": "#/$defs/ranked_file"}},
            "costs": {"$ref": "#/$defs/costs"},
            "overlays": {"type": "object"},
            "health": {"type": "object"},
            "diagnostics": {"type": "object"},
            "collection_metadata": {"type": "object"},
            "evidence_completeness": {"type": "object"}
            ,"terminology": {"type": "object", "required": ["attention_required", "budget_exceeded", "critical", "error"]}
        },
        "$defs": {
            "costs": {"type":"object","additionalProperties":false,"properties":{"load":{"type":"object","additionalProperties":false,"properties":{"file_token_count":{"type":"integer"},"folder_token_count":{"type":"integer"},"top_file_share":{"type":"number"},"top_3_file_share":{"type":"number"},"token_concentration_ratio":{"type":"number"},"context_band":{"type":"string"},"load_pressure":{"type":"number"}}},"volatility":{"type":"object","additionalProperties":false,"properties":{"commit_count_window":{"type":"number"},"recency_weighted_commits":{"type":"number"},"line_churn_window":{"type":"number"},"token_churn_window":{"type":"number"},"relative_token_churn":{"type":"number"},"late_churn_spike":{"type":"number"},"volatility_pressure":{"type":"number"},"churn_measurement":{"type":"string"}}},"coordination":{"type":"object","additionalProperties":false,"properties":{"files_touched_per_change":{"type":"number"},"folders_touched_per_change":{"type":"number"},"edit_hunks_per_change":{"type":"number"},"change_diffusion":{"type":"number"},"cochange_degree":{"type":"number"},"cochange_centrality":{"type":"number"},"cochange_pagerank":{"type":"number"},"cross_folder_cochange_ratio":{"type":"number"},"coordination_pressure":{"type":"number"}}}}},
            "compare_record": {"type":"object","additionalProperties":false,"required":["path","content_fingerprint","analysis_status","tokens","context_band","slop_score","slop_band","costs","overlays"],"properties":{"path":{"type":"string"},"content_fingerprint":{"type":["string","null"]},"analysis_status":{"type":"string"},"tokens":{"type":"integer","minimum":0},"context_band":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "file": {"type": "object", "additionalProperties": false, "required": ["path", "bytes", "lines", "blank_lines", "code_lines", "comment_lines", "language", "profile", "classification", "analysis_status", "skipped_reason", "symlink_metadata", "has_inline_tests", "tokens", "context_band", "context_pressure", "content_fingerprint", "structural_token_count", "top_structural_terms", "structural_categories", "age_days", "revisions_window", "recency_weighted_commits", "added_window", "deleted_window", "churn_lines_window", "line_churn_window", "token_churn_window", "relative_churn_window", "late_churn_spike", "author_count_window", "author_entropy", "top_author_share", "days_since_non_bot_edit", "recent_maintainer_diversity", "age_pressure", "revision_norm", "relative_churn_norm", "churn_pressure", "slop_score", "slop_band", "reason_codes", "costs", "overlays"], "properties": {"path":{"type":"string"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"blank_lines":{"type":"integer"},"code_lines":{"type":"integer"},"comment_lines":{"type":"integer"},"language":{"type":"string"},"profile":{"type":"string"},"classification":{"type":"string"},"analysis_status":{"type":"string"},"skipped_reason":{"type":["string","null"]},"symlink_metadata":{"type":["object","null"]},"has_inline_tests":{"type":"boolean"},"tokens":{"type":"integer"},"context_band":{"type":"string"},"context_pressure":{"type":"number"},"content_fingerprint":{"type":"string"},"structural_token_count":{"type":"integer"},"top_structural_terms":{"type":"array"},"structural_categories":{"type":"object"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"recency_weighted_commits":{"type":"number"},"added_window":{"type":"integer"},"deleted_window":{"type":"integer"},"churn_lines_window":{"type":"integer"},"line_churn_window":{"type":"integer"},"token_churn_window":{"type":"integer"},"relative_churn_window":{"type":"number"},"late_churn_spike":{"type":"number"},"author_count_window":{"type":"integer"},"author_entropy":{"type":"number"},"top_author_share":{"type":"number"},"days_since_non_bot_edit":{"type":["integer","null"]},"recent_maintainer_diversity":{"type":"integer"},"age_pressure":{"type":"number"},"revision_norm":{"type":"number"},"relative_churn_norm":{"type":"number"},"churn_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "folder": {"type": "object", "additionalProperties": false, "required": ["path", "descendant_file_count", "direct_file_count", "bytes", "lines", "tokens", "direct_tokens", "context_band", "health_band", "context_pressure", "slop_score", "slop_band", "reason_codes", "top_file_path", "classification", "costs", "overlays"], "properties":{"path":{"type":"string"},"descendant_file_count":{"type":"integer"},"direct_file_count":{"type":"integer"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"tokens":{"type":"integer"},"direct_tokens":{"type":"integer"},"context_band":{"type":"string"},"health_band":{"type":"string"},"context_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"top_file_path":{"type":"string"},"classification":{"type":"string"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "ranked_file": {"type": "object", "additionalProperties": false, "required": ["path", "slop_score", "slop_band", "context_band", "tokens", "reason_codes"], "properties":{"path":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"tokens":{"type":"integer"},"reason_codes":{"type":"array","items":{"type":"string"}}}}
        }
    })
}

pub fn load_report(path: &Path) -> Result<Value> {
    load_report_with_legacy(path, true)
}

pub fn load_report_with_legacy(path: &Path, allow_legacy: bool) -> Result<Value> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let report: Value = serde_json::from_str(&source)
        .with_context(|| format!("invalid git-slop report JSON: {}", path.display()))?;
    let schema_version = report.get("schema_version").and_then(Value::as_u64);
    let report = if schema_version == Some(4) {
        if !allow_legacy {
            anyhow::bail!(
                "legacy report schema 4 requires --allow-legacy or `git slop report migrate`"
            );
        }
        migrate_legacy_report(report)?
    } else {
        report
    };
    validate_report_shape(&report)
        .with_context(|| format!("invalid git-slop report shape: {}", path.display()))?;
    Ok(report)
}

pub fn migrate_legacy_report(mut report: Value) -> Result<Value> {
    if report.get("schema_version").and_then(Value::as_u64) != Some(4) {
        anyhow::bail!("only report schema 4 can be migrated to schema 5");
    }
    let legacy_relationships = report.get("relationships").cloned();
    let legacy_clusters = report.get("clusters").cloned();
    let legacy_metrics = report.get("organization_metrics").cloned();
    let root = report
        .as_object_mut()
        .ok_or_else(|| anyhow!("report root must be an object"))?;
    root.insert("schema_version".to_string(), json!(5));
    let zero_digest = "0".repeat(64);
    let analyzer = root
        .entry("analyzer")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("analyzer must be an object"))?;
    analyzer.entry("name").or_insert_with(|| json!("git-slop"));
    analyzer
        .entry("version")
        .or_insert_with(|| json!("legacy-schema-4"));
    analyzer
        .entry("report_profile")
        .or_insert_with(|| json!("standard"));
    analyzer
        .entry("analysis_contract_version")
        .or_insert_with(|| json!(1));
    for key in [
        "config_digest",
        "analysis_config_digest",
        "evidence_config_digest",
        "policy_config_digest",
        "presentation_config_digest",
    ] {
        analyzer
            .entry(key)
            .or_insert_with(|| json!(zero_digest.clone()));
    }
    analyzer
        .entry("context_tokenizer")
        .or_insert_with(|| json!("unknown"));
    if let Some(repo) = root.get_mut("repo").and_then(Value::as_object_mut) {
        repo.remove("repo_root");
        let legacy_head = repo.remove("head_commit");
        let legacy_remote = repo.remove("git_remote_url");
        repo.entry("repository_id").or_insert(Value::Null);
        repo.entry("repository_identity_source")
            .or_insert(Value::Null);
        repo.entry("branch").or_insert(Value::Null);
        repo.entry("head_commit_timestamp").or_insert(Value::Null);
        repo.entry("is_shallow").or_insert_with(|| json!(false));
        repo.entry("detached_head").or_insert_with(|| json!(false));
        repo.entry("worktree_clean").or_insert_with(|| json!(false));
        repo.entry("staged_change_count")
            .or_insert_with(|| json!(0));
        repo.entry("modified_tracked_file_count")
            .or_insert_with(|| json!(0));
        repo.entry("untracked_file_count")
            .or_insert_with(|| json!(0));
        repo.entry("worktree_state_digest")
            .or_insert_with(|| json!(zero_digest.clone()));
        repo.entry("analyzed_content_digest")
            .or_insert_with(|| json!(zero_digest.clone()));
        repo.entry("head_sha")
            .or_insert_with(|| legacy_head.unwrap_or(Value::Null));
        repo.entry("remote_url")
            .or_insert_with(|| legacy_remote.unwrap_or(Value::Null));
        let has_head = repo.get("head_sha").is_some_and(|value| !value.is_null());
        repo.entry("has_head_commit")
            .or_insert_with(|| json!(has_head));
    }
    root.entry("generated_at")
        .or_insert_with(|| json!("1970-01-01T00:00:00Z"));
    root.entry("analyzed_revision_at").or_insert(Value::Null);
    let selected_path_count = root
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    root.entry("scope").or_insert_with(|| {
        json!({
            "mode": "repository",
            "path": null,
            "selected_path_count": selected_path_count,
            "selected_path_digest": zero_digest
        })
    });
    root.entry("diagnostics").or_insert_with(|| {
        json!({
            "migration": "schema_4",
            "evidence_limit": "legacy report omitted schema-5 diagnostics"
        })
    });
    root.entry("terminology").or_insert_with(|| json!({
        "attention_required": "A review is warranted; the detector does not prove a refactor is required.",
        "budget_exceeded": "A configured file or folder context budget was exceeded.",
        "critical": "The highest detector context or maintenance-pressure band.",
        "error": "A delivery severity used by CI annotations for budget-exceeded findings."
    }));
    root.entry("costs").or_insert_with(|| json!({}));
    let legacy_config = root.get("config").cloned().unwrap_or_else(|| json!({}));
    root.insert(
        "config".to_string(),
        crate::config::effective_from_override(legacy_config)
            .context("legacy embedded configuration cannot be normalized")?,
    );
    if let Some(files) = root.get_mut("files").and_then(Value::as_array_mut) {
        for file in files {
            if let Some(file) = file.as_object_mut() {
                let tokens = file
                    .get("costs")
                    .and_then(|value| value.pointer("/load/file_token_count"))
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                for (key, value) in [
                    ("bytes", json!(0)),
                    ("lines", json!(0)),
                    ("blank_lines", json!(0)),
                    ("code_lines", json!(0)),
                    ("comment_lines", json!(0)),
                    ("language", json!("Unknown")),
                    ("profile", json!("agent_context")),
                    ("classification", json!("other")),
                    ("has_inline_tests", json!(false)),
                    ("tokens", json!(tokens)),
                    ("context_pressure", json!(0.0)),
                    ("content_fingerprint", json!("")),
                    ("structural_token_count", json!(0)),
                    ("top_structural_terms", json!([])),
                    ("age_days", json!(0)),
                    ("revisions_window", json!(0)),
                    ("recency_weighted_commits", json!(0.0)),
                    ("added_window", json!(0)),
                    ("deleted_window", json!(0)),
                    ("churn_lines_window", json!(0)),
                    ("line_churn_window", json!(0)),
                    ("token_churn_window", json!(0)),
                    ("relative_churn_window", json!(0.0)),
                    ("late_churn_spike", json!(0.0)),
                    ("author_count_window", json!(0)),
                    ("author_entropy", json!(0.0)),
                    ("top_author_share", json!(0.0)),
                    ("days_since_non_bot_edit", Value::Null),
                    ("recent_maintainer_diversity", json!(0)),
                    ("age_pressure", json!(0.0)),
                    ("revision_norm", json!(0.0)),
                    ("relative_churn_norm", json!(0.0)),
                    ("churn_pressure", json!(0.0)),
                    ("context_band", json!("compact")),
                    ("slop_score", json!(0.0)),
                    ("slop_band", json!("low")),
                    ("reason_codes", json!([])),
                    ("costs", json!({})),
                    ("overlays", json!({})),
                ] {
                    file.entry(key).or_insert(value);
                }
                file.entry("analysis_status")
                    .or_insert_with(|| json!("analyzed"));
                file.entry("skipped_reason").or_insert(Value::Null);
                file.entry("structural_categories")
                    .or_insert_with(|| json!({"mode": "legacy_unknown"}));
                file.entry("symlink_metadata").or_insert(Value::Null);
                if let Some(overlays) = file.get_mut("overlays").and_then(Value::as_object_mut) {
                    if let Some(mut concept) = overlays.remove("semantic_drift") {
                        if let Some(object) = concept.as_object_mut() {
                            if let Some(value) = object.remove("semantic_drift_pressure") {
                                object.insert("concept_dispersion_pressure".to_string(), value);
                            }
                        }
                        overlays.insert("concept_dispersion".to_string(), concept);
                    }
                }
            }
        }
    }
    if let Some(folders) = root.get_mut("folders").and_then(Value::as_array_mut) {
        for folder in folders {
            if let Some(folder) = folder.as_object_mut() {
                let tokens = folder
                    .get("costs")
                    .and_then(|value| value.pointer("/load/folder_token_count"))
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                for (key, value) in [
                    ("descendant_file_count", json!(0)),
                    ("direct_file_count", json!(0)),
                    ("bytes", json!(0)),
                    ("lines", json!(0)),
                    ("tokens", json!(tokens)),
                    ("direct_tokens", json!(tokens)),
                    ("health_band", json!("compact")),
                    ("context_pressure", json!(0.0)),
                    ("top_file_path", json!("")),
                    ("classification", json!("other")),
                    ("context_band", json!("compact")),
                    ("slop_score", json!(0.0)),
                    ("slop_band", json!("low")),
                    ("reason_codes", json!([])),
                    ("costs", json!({})),
                    ("overlays", json!({})),
                ] {
                    folder.entry(key).or_insert(value);
                }
                if let Some(overlays) = folder.get_mut("overlays").and_then(Value::as_object_mut) {
                    if let Some(mut concept) = overlays.remove("semantic_drift") {
                        if let Some(object) = concept.as_object_mut() {
                            if let Some(value) = object.remove("semantic_drift_pressure") {
                                object.insert("concept_dispersion_pressure".to_string(), value);
                            }
                        }
                        overlays.insert("concept_dispersion".to_string(), concept);
                    }
                }
            }
        }
    }
    let files_by_path = root
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| {
            file.get("path")
                .and_then(Value::as_str)
                .map(|path| (path.to_string(), file.clone()))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    if let Some(queue) = root.get_mut("action_queue").and_then(Value::as_array_mut) {
        for item in queue {
            let Some(object) = item.as_object_mut() else {
                continue;
            };
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let source = files_by_path
                .get(path)
                .cloned()
                .unwrap_or_else(|| json!({}));
            let reasons = object
                .get("reason_codes")
                .or_else(|| source.get("reason_codes"))
                .cloned()
                .unwrap_or_else(|| json!([]));
            for (key, value) in [
                (
                    "profile",
                    source
                        .get("profile")
                        .cloned()
                        .unwrap_or_else(|| json!("agent_context")),
                ),
                (
                    "slop_score",
                    source
                        .get("slop_score")
                        .cloned()
                        .unwrap_or_else(|| json!(0.0)),
                ),
                (
                    "slop_band",
                    source
                        .get("slop_band")
                        .cloned()
                        .unwrap_or_else(|| json!("low")),
                ),
                (
                    "context_band",
                    source
                        .get("context_band")
                        .cloned()
                        .unwrap_or_else(|| json!("compact")),
                ),
                (
                    "tokens",
                    source.get("tokens").cloned().unwrap_or_else(|| json!(0)),
                ),
                (
                    "age_days",
                    source.get("age_days").cloned().unwrap_or_else(|| json!(0)),
                ),
                (
                    "revisions_window",
                    source
                        .get("revisions_window")
                        .cloned()
                        .unwrap_or_else(|| json!(0)),
                ),
                (
                    "churn_pressure",
                    source
                        .get("churn_pressure")
                        .cloned()
                        .unwrap_or_else(|| json!(0.0)),
                ),
                ("reason_codes", reasons.clone()),
                (
                    "is_pure_context_hotspot",
                    json!(reasons.as_array().is_some_and(|values| !values.is_empty()
                        && values.iter().all(|reason| matches!(
                            reason.as_str(),
                            Some("high_token_cost" | "critical_token_cost")
                        )))),
                ),
                ("severity", json!("notice")),
                ("evidence_status", json!("legacy_unknown")),
                (
                    "next_action",
                    json!(format!(
                        "git slop explain --path {}",
                        source
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    )),
                ),
            ] {
                object.entry(key).or_insert(value);
            }
        }
    }
    let ranked_files = root
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|file| {
            json!({
                "path": file.get("path").cloned().unwrap_or(Value::Null),
                "slop_score": file.get("slop_score").cloned().unwrap_or_else(|| json!(0.0)),
                "slop_band": file.get("slop_band").cloned().unwrap_or_else(|| json!("low")),
                "context_band": file.get("context_band").cloned().unwrap_or_else(|| json!("compact")),
                "tokens": file.get("tokens").cloned().unwrap_or_else(|| json!(0)),
                "reason_codes": file.get("reason_codes").cloned().unwrap_or_else(|| json!([]))
            })
        })
        .collect::<Vec<_>>();
    root.entry("ranked_files")
        .or_insert_with(|| json!(ranked_files));
    root.entry("collection_metadata")
        .or_insert_with(|| json!({}));
    let comparison_record = |record: &Value| {
        let overlays = record.get("overlays").unwrap_or(&Value::Null);
        json!({
            "path": record.get("path").cloned().unwrap_or(Value::Null),
            "content_fingerprint": record.get("content_fingerprint").cloned().unwrap_or(Value::Null),
            "analysis_status": record.get("analysis_status").cloned().unwrap_or_else(|| json!("legacy_unknown")),
            "tokens": record.get("tokens").cloned().unwrap_or_else(|| json!(0)),
            "context_band": record.get("context_band").cloned().unwrap_or_else(|| json!("compact")),
            "slop_score": record.get("slop_score").cloned().unwrap_or_else(|| json!(0.0)),
            "slop_band": record.get("slop_band").cloned().unwrap_or_else(|| json!("low")),
            "overlays": {
                "organization_health": {
                    "duplication_pressure": overlays.pointer("/organization_health/duplication_pressure").cloned().unwrap_or(Value::Null),
                    "diffusion_pressure": overlays.pointer("/organization_health/diffusion_pressure").cloned().unwrap_or(Value::Null),
                    "coupling_pressure": overlays.pointer("/organization_health/coupling_pressure").cloned().unwrap_or(Value::Null),
                    "boundary_pressure": overlays.pointer("/organization_health/boundary_pressure").cloned().unwrap_or(Value::Null)
                },
                "verification": {"verification_gap": overlays.pointer("/verification/verification_gap").cloned().unwrap_or(Value::Null)},
                "navigation": {"navigation_pressure": overlays.pointer("/navigation/navigation_pressure").cloned().unwrap_or(Value::Null)},
                "blast_radius": {"blast_radius_pressure": overlays.pointer("/blast_radius/blast_radius_pressure").cloned().unwrap_or(Value::Null)},
                "stewardship": {"stewardship_pressure": overlays.pointer("/stewardship/stewardship_pressure").cloned().unwrap_or(Value::Null)},
                "concept_dispersion": {"concept_dispersion_pressure": overlays.pointer("/concept_dispersion/concept_dispersion_pressure").cloned().unwrap_or(Value::Null)}
            },
            "costs": {"load": {"load_pressure": record.pointer("/costs/load/load_pressure").cloned().unwrap_or_else(|| json!(0.0))}}
        })
    };
    let compare_files = root
        .get("files")
        .and_then(Value::as_array)
        .map(|records| records.iter().map(&comparison_record).collect::<Vec<_>>())
        .unwrap_or_default();
    let compare_folders = root
        .get("folders")
        .and_then(Value::as_array)
        .map(|records| records.iter().map(comparison_record).collect::<Vec<_>>())
        .unwrap_or_default();
    root.entry("compare_index")
        .or_insert_with(|| json!({"files": compare_files, "folders": compare_folders}));
    let compare_file_count = root
        .get("compare_index")
        .and_then(|value| value.get("files"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let compare_folder_count = root
        .get("compare_index")
        .and_then(|value| value.get("folders"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let collection_metadata = root
        .get_mut("collection_metadata")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("collection_metadata must be an object"))?;
    if !collection_metadata.contains_key("compare_index") {
        let files_metadata = collection_metadata
            .get("files")
            .cloned()
            .unwrap_or_else(|| json!({"total": compare_file_count, "returned": compare_file_count, "limit": null, "truncated": false}));
        let folders_metadata = collection_metadata
            .get("folders")
            .cloned()
            .unwrap_or_else(|| json!({"total": compare_folder_count, "returned": compare_folder_count, "limit": null, "truncated": false}));
        collection_metadata.insert(
            "compare_index".to_string(),
            json!({"files": files_metadata, "folders": folders_metadata}),
        );
    }
    root.entry("evidence_completeness").or_insert_with(|| {
        json!({
            "history": "legacy_unknown",
            "repository_size": "legacy_unknown",
            "relationship_evidence": "legacy_unknown"
        })
    });
    if root
        .get("evidence_completeness")
        .and_then(Value::as_object)
        .is_some_and(serde_json::Map::is_empty)
    {
        root.insert(
            "evidence_completeness".to_string(),
            json!({
                "history": "legacy_unknown",
                "repository_size": "legacy_unknown",
                "relationship_evidence": "legacy_unknown"
            }),
        );
    }
    if root
        .get("stats")
        .and_then(Value::as_object)
        .is_some_and(serde_json::Map::is_empty)
    {
        root.insert(
            "stats".to_string(),
            json!({"migration_status": "legacy_unknown"}),
        );
    }
    let overlays = root
        .entry("overlays")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("overlays must be an object"))?;
    let organization = overlays
        .entry("organization_health")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("overlays.organization_health must be an object"))?;
    if let Some(value) = legacy_relationships {
        organization.entry("relationships").or_insert(value);
    }
    if let Some(value) = legacy_clusters {
        organization.entry("clusters").or_insert(value);
    }
    if let Some(value) = legacy_metrics {
        organization.entry("organization_metrics").or_insert(value);
    }
    root.remove("relationships");
    root.remove("clusters");
    root.remove("organization_metrics");
    let migrated_snapshot = Value::Object(root.clone());
    let health = crate::health::health_rollup_from_report(&migrated_snapshot)
        .context("legacy health evidence cannot be normalized")?;
    root.insert("health".to_string(), serde_json::to_value(health)?);
    Ok(Value::Object(root.clone()))
}

#[derive(Debug, Serialize)]
struct ValidationIssue {
    code: &'static str,
    pointer: String,
    message: String,
}

fn collect_unknown_fields(
    issues: &mut Vec<ValidationIssue>,
    value: Option<&Value>,
    pointer: &str,
    allowed: &[&str],
) {
    let Some(object) = value.and_then(Value::as_object) else {
        return;
    };
    for field in object
        .keys()
        .filter(|field| !allowed.contains(&field.as_str()))
    {
        issues.push(ValidationIssue {
            code: "unknown_field",
            pointer: format!("{pointer}/{field}"),
            message: format!("unknown field {field:?}"),
        });
    }
}

fn validation_issues(report: &Value) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let Some(root) = report.as_object() else {
        return vec![ValidationIssue {
            code: "type_mismatch",
            pointer: String::new(),
            message: "report root must be an object".to_string(),
        }];
    };
    let required = [
        "schema_version",
        "analyzer",
        "generated_at",
        "analyzed_revision_at",
        "repo",
        "scope",
        "config",
        "stats",
        "summary",
        "files",
        "folders",
        "ranked_files",
        "action_queue",
        "costs",
        "overlays",
        "health",
        "diagnostics",
        "collection_metadata",
        "evidence_completeness",
        "terminology",
    ];
    let allowed_root = [
        "schema_version",
        "analyzer",
        "generated_at",
        "analyzed_revision_at",
        "repo",
        "scope",
        "config",
        "stats",
        "summary",
        "costs",
        "files",
        "folders",
        "compare_index",
        "ranked_files",
        "action_queue",
        "costs",
        "overlays",
        "health",
        "diagnostics",
        "collection_metadata",
        "evidence_completeness",
        "terminology",
    ];
    for key in root
        .keys()
        .filter(|key| !allowed_root.contains(&key.as_str()))
    {
        issues.push(ValidationIssue {
            code: "unknown_field",
            pointer: format!("/{key}"),
            message: format!("unknown report field {key:?}"),
        });
    }
    for key in required {
        if !root.contains_key(key) {
            issues.push(ValidationIssue {
                code: "required_field_missing",
                pointer: format!("/{key}"),
                message: format!("required field {key:?} is missing"),
            });
        }
    }
    if root.get("schema_version").and_then(Value::as_u64) != Some(5) {
        issues.push(ValidationIssue {
            code: "unsupported_schema_version",
            pointer: "/schema_version".to_string(),
            message: "schema_version must be 5".to_string(),
        });
    }
    for key in [
        "analyzer",
        "repo",
        "scope",
        "config",
        "stats",
        "summary",
        "costs",
        "overlays",
        "health",
        "diagnostics",
        "collection_metadata",
        "evidence_completeness",
        "terminology",
    ] {
        if root.get(key).is_some_and(|value| !value.is_object()) {
            issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: format!("/{key}"),
                message: format!("{key} must be an object"),
            });
        }
    }
    for key in ["stats", "diagnostics", "evidence_completeness"] {
        if root
            .get(key)
            .and_then(Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
        {
            issues.push(ValidationIssue {
                code: "empty_evidence_object",
                pointer: format!("/{key}"),
                message: format!("{key} must contain explicit status or evidence fields"),
            });
        }
    }
    collect_unknown_fields(
        &mut issues,
        root.get("diagnostics"),
        "/diagnostics",
        &[
            "analysis",
            "evidence_limit",
            "migration",
            "relationship_count",
            "report_profile",
            "report_profile_semantics",
            "report_sizes",
            "structural_token_payload_omitted",
            "suppressed_saturated_overlays",
        ],
    );
    collect_unknown_fields(
        &mut issues,
        report.pointer("/diagnostics/analysis"),
        "/diagnostics/analysis",
        &[
            "analysis_elapsed_ms_before_report",
            "analysis_status",
            "cache_bytes",
            "cache_cleanup_warnings",
            "cache_entries",
            "cache_failed_evictions",
            "cache_hits",
            "cache_misses",
            "cache_status",
            "degraded_omitted_path_count",
            "estimate",
            "estimator_error_ratio",
            "estimate_range_contains_measurement",
            "history",
            "history_evidence_status",
            "measured_peak_rss_bytes",
            "memory_budget_exceeded_checkpoints",
            "memory_measurement_status",
            "original_selected_path_count",
            "resource_mode",
            "scope",
            "structurally_skipped_large_files",
        ],
    );
    let cost_groups = ["load", "volatility", "coordination"];
    let load_fields = [
        "file_token_count",
        "folder_token_count",
        "top_file_share",
        "top_3_file_share",
        "token_concentration_ratio",
        "context_band",
        "load_pressure",
    ];
    let volatility_fields = [
        "commit_count_window",
        "recency_weighted_commits",
        "line_churn_window",
        "token_churn_window",
        "relative_token_churn",
        "late_churn_spike",
        "volatility_pressure",
        "churn_measurement",
    ];
    let coordination_fields = [
        "files_touched_per_change",
        "folders_touched_per_change",
        "edit_hunks_per_change",
        "change_diffusion",
        "cochange_degree",
        "cochange_centrality",
        "cochange_pagerank",
        "cross_folder_cochange_ratio",
        "coordination_pressure",
    ];
    for (collection, values) in [
        ("files", root.get("files")),
        ("folders", root.get("folders")),
    ] {
        for (index, record) in values
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let pointer = format!("/{collection}/{index}/costs");
            collect_unknown_fields(&mut issues, record.get("costs"), &pointer, &cost_groups);
            collect_unknown_fields(
                &mut issues,
                record.pointer("/costs/load"),
                &format!("{pointer}/load"),
                &load_fields,
            );
            collect_unknown_fields(
                &mut issues,
                record.pointer("/costs/volatility"),
                &format!("{pointer}/volatility"),
                &volatility_fields,
            );
            collect_unknown_fields(
                &mut issues,
                record.pointer("/costs/coordination"),
                &format!("{pointer}/coordination"),
                &coordination_fields,
            );
        }
    }
    let relationship_groups = [
        "analysis_status",
        "analysis_version",
        "duplicate_neighborhoods",
        "near_duplicate_neighborhoods",
        "temporal_coupling_edges",
        "lexical_affinity_edges",
        "boundary_leakage_edges",
        "diagnostics",
    ];
    let relationships = report.pointer("/overlays/organization_health/relationships");
    collect_unknown_fields(
        &mut issues,
        relationships,
        "/overlays/organization_health/relationships",
        &relationship_groups,
    );
    let relationship_fields = [
        "id",
        "kind",
        "source_path",
        "target_path",
        "evidence_score",
        "similarity",
        "crosses_top_level_boundary",
        "support_count",
        "calibrated_support",
        "creation_support_count",
        "maintenance_support_count",
        "source_commit_count",
        "target_commit_count",
        "observation_commit_count",
        "source_confidence",
        "target_confidence",
        "jaccard",
        "calibrated_jaccard",
        "lift_score",
        "confidence_lower_bound",
        "confidence",
        "similarity_ratio",
        "duplicate_token_mass",
    ];
    for group in [
        "duplicate_neighborhoods",
        "near_duplicate_neighborhoods",
        "temporal_coupling_edges",
        "lexical_affinity_edges",
        "boundary_leakage_edges",
    ] {
        for (index, relationship) in relationships
            .and_then(|value| value.get(group))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            collect_unknown_fields(
                &mut issues,
                Some(relationship),
                &format!("/overlays/organization_health/relationships/{group}/{index}"),
                &relationship_fields,
            );
        }
    }
    if let Some(timestamp) = root.get("generated_at") {
        if timestamp
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_none()
        {
            issues.push(ValidationIssue {
                code: "invalid_timestamp",
                pointer: "/generated_at".to_string(),
                message: "generated_at must be an RFC 3339 timestamp".to_string(),
            });
        }
    }
    if let Some(timestamp) = root
        .get("analyzed_revision_at")
        .filter(|value| !value.is_null())
    {
        if timestamp
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_none()
        {
            issues.push(ValidationIssue {
                code: "invalid_timestamp",
                pointer: "/analyzed_revision_at".to_string(),
                message: "analyzed_revision_at must be null or an RFC 3339 timestamp".to_string(),
            });
        }
    }
    if let Some(analyzer) = root.get("analyzer").and_then(Value::as_object) {
        let allowed = [
            "name",
            "version",
            "report_profile",
            "analysis_clock",
            "analysis_contract_version",
            "config_digest",
            "analysis_config_digest",
            "evidence_config_digest",
            "policy_config_digest",
            "presentation_config_digest",
            "context_tokenizer",
        ];
        for key in analyzer
            .keys()
            .filter(|key| !allowed.contains(&key.as_str()))
        {
            issues.push(ValidationIssue {
                code: "unknown_field",
                pointer: format!("/analyzer/{key}"),
                message: format!("unknown analyzer field {key:?}"),
            });
        }
        if analyzer.get("analysis_clock").is_some_and(|value| {
            value
                .as_str()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .is_none()
        }) {
            issues.push(ValidationIssue {
                code: "invalid_timestamp",
                pointer: "/analyzer/analysis_clock".to_string(),
                message: "analyzer.analysis_clock must be an RFC 3339 timestamp".to_string(),
            });
        }
        for key in [
            "name",
            "version",
            "report_profile",
            "config_digest",
            "analysis_config_digest",
            "evidence_config_digest",
            "policy_config_digest",
            "presentation_config_digest",
            "context_tokenizer",
        ] {
            if analyzer.get(key).and_then(Value::as_str).is_none() {
                issues.push(ValidationIssue {
                    code: "type_mismatch",
                    pointer: format!("/analyzer/{key}"),
                    message: format!("analyzer.{key} must be a string"),
                });
            }
        }
        if analyzer
            .get("analysis_contract_version")
            .and_then(Value::as_u64)
            .is_none()
        {
            issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: "/analyzer/analysis_contract_version".to_string(),
                message: "analysis_contract_version must be an integer".to_string(),
            });
        }
        for key in [
            "config_digest",
            "analysis_config_digest",
            "evidence_config_digest",
            "policy_config_digest",
            "presentation_config_digest",
        ] {
            if analyzer
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
            {
                issues.push(ValidationIssue {
                    code: "invalid_digest",
                    pointer: format!("/analyzer/{key}"),
                    message: format!("analyzer.{key} must be a 64-character hexadecimal digest"),
                });
            }
        }
    }
    if let Some(repo) = root.get("repo").and_then(Value::as_object) {
        let fields = [
            "repo_name",
            "repository_id",
            "repository_identity_source",
            "branch",
            "head_commit_timestamp",
            "is_shallow",
            "detached_head",
            "worktree_clean",
            "staged_change_count",
            "modified_tracked_file_count",
            "untracked_file_count",
            "worktree_state_digest",
            "analyzed_content_digest",
            "head_sha",
            "remote_url",
            "has_head_commit",
        ];
        for field in fields {
            if !repo.contains_key(field) {
                issues.push(ValidationIssue {
                    code: "required_field_missing",
                    pointer: format!("/repo/{field}"),
                    message: format!("required repository field {field:?} is missing"),
                });
            }
        }
        for field in repo
            .keys()
            .filter(|field| !fields.contains(&field.as_str()))
        {
            issues.push(ValidationIssue {
                code: "unknown_field",
                pointer: format!("/repo/{field}"),
                message: format!("unknown repository field {field:?}"),
            });
        }
        for field in ["repo_name", "worktree_state_digest"] {
            if repo.get(field).and_then(Value::as_str).is_none() {
                issues.push(ValidationIssue {
                    code: "type_mismatch",
                    pointer: format!("/repo/{field}"),
                    message: format!("repo.{field} must be a string"),
                });
            }
        }
        for field in [
            "repository_id",
            "repository_identity_source",
            "branch",
            "head_commit_timestamp",
            "analyzed_content_digest",
            "head_sha",
            "remote_url",
        ] {
            if repo
                .get(field)
                .is_some_and(|value| !value.is_null() && !value.is_string())
            {
                issues.push(ValidationIssue {
                    code: "type_mismatch",
                    pointer: format!("/repo/{field}"),
                    message: format!("repo.{field} must be a string or null"),
                });
            }
        }
        for field in [
            "is_shallow",
            "detached_head",
            "worktree_clean",
            "has_head_commit",
        ] {
            if repo.get(field).is_some_and(|value| !value.is_boolean()) {
                issues.push(ValidationIssue {
                    code: "type_mismatch",
                    pointer: format!("/repo/{field}"),
                    message: format!("repo.{field} must be a boolean"),
                });
            }
        }
        for field in [
            "staged_change_count",
            "modified_tracked_file_count",
            "untracked_file_count",
        ] {
            if repo
                .get(field)
                .is_some_and(|value| value.as_u64().is_none())
            {
                issues.push(ValidationIssue {
                    code: "type_mismatch",
                    pointer: format!("/repo/{field}"),
                    message: format!("repo.{field} must be a nonnegative integer"),
                });
            }
        }
        if repo.get("head_commit_timestamp").is_some_and(|value| {
            !value.is_null()
                && value
                    .as_str()
                    .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
                    .is_none()
        }) {
            issues.push(ValidationIssue {
                code: "invalid_timestamp",
                pointer: "/repo/head_commit_timestamp".to_string(),
                message: "repo.head_commit_timestamp must be null or an RFC 3339 timestamp"
                    .to_string(),
            });
        }
    }
    if let Some(scope) = root.get("scope").and_then(Value::as_object) {
        let fields = [
            "mode",
            "path",
            "selected_path_count",
            "selected_path_digest",
        ];
        for field in fields {
            if !scope.contains_key(field) {
                issues.push(ValidationIssue {
                    code: "required_field_missing",
                    pointer: format!("/scope/{field}"),
                    message: format!("required scope field {field:?} is missing"),
                });
            }
        }
        for field in scope
            .keys()
            .filter(|field| !fields.contains(&field.as_str()))
        {
            issues.push(ValidationIssue {
                code: "unknown_field",
                pointer: format!("/scope/{field}"),
                message: format!("unknown scope field {field:?}"),
            });
        }
        if !matches!(
            scope.get("mode").and_then(Value::as_str),
            Some("repository" | "scoped")
        ) {
            issues.push(ValidationIssue {
                code: "invalid_enum",
                pointer: "/scope/mode".to_string(),
                message: "scope.mode must be repository or scoped".to_string(),
            });
        }
        if scope
            .get("path")
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: "/scope/path".to_string(),
                message: "scope.path must be a string or null".to_string(),
            });
        }
        if scope
            .get("selected_path_count")
            .is_some_and(|value| value.as_u64().is_none())
        {
            issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: "/scope/selected_path_count".to_string(),
                message: "scope.selected_path_count must be a nonnegative integer".to_string(),
            });
        }
    }
    for key in ["files", "folders", "ranked_files", "action_queue"] {
        match root.get(key).and_then(Value::as_array) {
            Some(records) => {
                for (index, record) in records.iter().enumerate() {
                    if !record.is_object() {
                        issues.push(ValidationIssue {
                            code: "type_mismatch",
                            pointer: format!("/{key}/{index}"),
                            message: "record must be an object".to_string(),
                        });
                    } else if record.get("path").and_then(Value::as_str).is_none() {
                        issues.push(ValidationIssue {
                            code: "required_field_missing",
                            pointer: format!("/{key}/{index}/path"),
                            message: "path must be a string".to_string(),
                        });
                    }
                }
            }
            None if root.contains_key(key) => issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: format!("/{key}"),
                message: format!("{key} must be an array"),
            }),
            None => {}
        }
    }
    if let Some(compare_index) = root.get("compare_index") {
        let Some(compare_index) = compare_index.as_object() else {
            issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: "/compare_index".to_string(),
                message: "compare_index must be an object".to_string(),
            });
            return issues;
        };
        for collection in ["files", "folders"] {
            match compare_index.get(collection).and_then(Value::as_array) {
                Some(records) => {
                    for (index, record) in records.iter().enumerate() {
                        if record.get("path").and_then(Value::as_str).is_none() {
                            issues.push(ValidationIssue {
                                code: "required_field_missing",
                                pointer: format!("/compare_index/{collection}/{index}/path"),
                                message: "path must be a string".to_string(),
                            });
                        }
                    }
                }
                None => issues.push(ValidationIssue {
                    code: "type_mismatch",
                    pointer: format!("/compare_index/{collection}"),
                    message: format!("compare_index.{collection} must be an array"),
                }),
            }
        }
    }
    let file_fields = [
        "path",
        "bytes",
        "lines",
        "blank_lines",
        "code_lines",
        "comment_lines",
        "language",
        "profile",
        "classification",
        "analysis_status",
        "skipped_reason",
        "symlink_metadata",
        "has_inline_tests",
        "tokens",
        "context_band",
        "context_pressure",
        "content_fingerprint",
        "structural_token_count",
        "top_structural_terms",
        "structural_categories",
        "age_days",
        "revisions_window",
        "recency_weighted_commits",
        "added_window",
        "deleted_window",
        "churn_lines_window",
        "line_churn_window",
        "token_churn_window",
        "relative_churn_window",
        "late_churn_spike",
        "author_count_window",
        "author_entropy",
        "top_author_share",
        "days_since_non_bot_edit",
        "recent_maintainer_diversity",
        "age_pressure",
        "revision_norm",
        "relative_churn_norm",
        "churn_pressure",
        "slop_score",
        "slop_band",
        "reason_codes",
        "costs",
        "overlays",
    ];
    let folder_fields = [
        "path",
        "descendant_file_count",
        "direct_file_count",
        "bytes",
        "lines",
        "tokens",
        "direct_tokens",
        "context_band",
        "health_band",
        "context_pressure",
        "slop_score",
        "slop_band",
        "reason_codes",
        "top_file_path",
        "classification",
        "costs",
        "overlays",
    ];
    for (collection, fields) in [
        ("files", file_fields.as_slice()),
        ("folders", folder_fields.as_slice()),
    ] {
        for (index, record) in root
            .get(collection)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(object) = record.as_object() else {
                continue;
            };
            for field in fields {
                if !object.contains_key(*field) {
                    issues.push(ValidationIssue {
                        code: "required_field_missing",
                        pointer: format!("/{collection}/{index}/{field}"),
                        message: format!("required field {field:?} is missing"),
                    });
                }
            }
            for field in object
                .keys()
                .filter(|field| !fields.contains(&field.as_str()))
            {
                issues.push(ValidationIssue {
                    code: "unknown_field",
                    pointer: format!("/{collection}/{index}/{field}"),
                    message: format!("unknown {collection} field {field:?}"),
                });
            }
        }
    }
    let queue_fields = [
        "path",
        "profile",
        "slop_score",
        "slop_band",
        "context_band",
        "tokens",
        "age_days",
        "revisions_window",
        "churn_pressure",
        "reason_codes",
        "is_pure_context_hotspot",
        "severity",
        "evidence_status",
        "next_action",
    ];
    let ranked_fields = [
        "path",
        "slop_score",
        "slop_band",
        "context_band",
        "tokens",
        "reason_codes",
    ];
    for (collection, fields) in [
        ("action_queue", queue_fields.as_slice()),
        ("ranked_files", ranked_fields.as_slice()),
    ] {
        for (index, record) in root
            .get(collection)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(object) = record.as_object() else {
                continue;
            };
            for field in fields {
                if !object.contains_key(*field) {
                    issues.push(ValidationIssue {
                        code: "required_field_missing",
                        pointer: format!("/{collection}/{index}/{field}"),
                        message: format!("required field {field:?} is missing"),
                    });
                }
            }
            for field in object
                .keys()
                .filter(|field| !fields.contains(&field.as_str()))
            {
                issues.push(ValidationIssue {
                    code: "unknown_field",
                    pointer: format!("/{collection}/{index}/{field}"),
                    message: format!("unknown {collection} field {field:?}"),
                });
            }
        }
    }
    if let Some(relationships) = root
        .get("overlays")
        .and_then(|value| value.pointer("/organization_health/relationships"))
    {
        if let Some(collections) = relationships.as_object() {
            for (collection, records) in collections {
                if matches!(
                    collection.as_str(),
                    "analysis_status" | "analysis_version" | "diagnostics"
                ) {
                    continue;
                }
                let Some(records) = records.as_array() else {
                    issues.push(ValidationIssue {
                        code: "type_mismatch",
                        pointer: format!(
                            "/overlays/organization_health/relationships/{collection}"
                        ),
                        message: "relationship collection must be an array".to_string(),
                    });
                    continue;
                };
                for (index, record) in records.iter().enumerate() {
                    for field in ["id", "kind", "source_path", "target_path", "evidence_score"] {
                        if record.get(field).is_none() {
                            issues.push(ValidationIssue { code: "required_field_missing", pointer: format!("/overlays/organization_health/relationships/{collection}/{index}/{field}"), message: format!("relationship field {field:?} is missing") });
                        }
                    }
                    if record
                        .get("evidence_score")
                        .is_some_and(|value| value.as_f64().is_none())
                    {
                        issues.push(ValidationIssue { code: "type_mismatch", pointer: format!("/overlays/organization_health/relationships/{collection}/{index}/evidence_score"), message: "relationship evidence_score must be a number".to_string() });
                    }
                }
            }
        } else {
            issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: "/overlays/organization_health/relationships".to_string(),
                message: "relationships must be an object".to_string(),
            });
        }
    }
    issues
}

pub fn validate_report_shape(report: &Value) -> Result<()> {
    let issues = validation_issues(report);
    if !issues.is_empty() {
        anyhow::bail!(
            "report validation failed: {}",
            serde_json::to_string(&issues).unwrap_or_else(|_| "[]".to_string())
        );
    }
    let Some(root) = report.as_object() else {
        anyhow::bail!("report root must be an object");
    };
    if root.get("schema_version").and_then(Value::as_u64) != Some(5) {
        anyhow::bail!("schema_version must be 5");
    }
    for key in ["repo", "config", "stats", "summary", "overlays", "health"] {
        if !root.get(key).is_some_and(Value::is_object) {
            anyhow::bail!("{key} must be an object");
        }
    }
    let repo = root["repo"].as_object().expect("repo checked as object");
    if repo.get("repo_name").and_then(Value::as_str).is_none() {
        anyhow::bail!("repo.repo_name must be a string");
    }
    for key in ["files", "folders", "ranked_files", "action_queue"] {
        let Some(records) = root.get(key).and_then(Value::as_array) else {
            anyhow::bail!("{key} must be an array");
        };
        for (index, record) in records.iter().enumerate() {
            if !record.is_object() {
                anyhow::bail!("{key}[{index}] must be an object");
            }
            if record.get("path").and_then(Value::as_str).is_none() {
                anyhow::bail!("{key}[{index}].path must be a string");
            }
        }
    }
    for (index, record) in root["files"]
        .as_array()
        .expect("files checked as array")
        .iter()
        .enumerate()
    {
        for (field, kind) in [
            ("tokens", "integer"),
            ("context_band", "string"),
            ("slop_score", "number"),
            ("slop_band", "string"),
            ("reason_codes", "array"),
            ("costs", "object"),
            ("overlays", "object"),
        ] {
            let Some(value) = record.get(field) else {
                anyhow::bail!("files[{index}].{field} is required by report schema 5");
            };
            let valid = match kind {
                "integer" => value.as_u64().is_some(),
                "number" => value.as_f64().is_some_and(f64::is_finite),
                "string" => value.is_string(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => false,
            };
            if !valid {
                anyhow::bail!("files[{index}].{field} must be a finite {kind}");
            }
        }
    }

    {
        for key in [
            "generated_at",
            "analyzed_revision_at",
            "scope",
            "evidence_completeness",
            "diagnostics",
            "costs",
            "collection_metadata",
        ] {
            if !root.contains_key(key) {
                anyhow::bail!("{key} is required in canonical schema-5 reports");
            }
        }
        let analyzer = root["analyzer"]
            .as_object()
            .ok_or_else(|| anyhow!("analyzer must be an object"))?;
        for key in [
            "name",
            "version",
            "report_profile",
            "config_digest",
            "analysis_config_digest",
            "evidence_config_digest",
            "policy_config_digest",
            "presentation_config_digest",
            "context_tokenizer",
        ] {
            if analyzer.get(key).and_then(Value::as_str).is_none() {
                anyhow::bail!("analyzer.{key} must be a string");
            }
        }
        if analyzer
            .get("analysis_contract_version")
            .and_then(Value::as_u64)
            .is_none()
        {
            anyhow::bail!("analyzer.analysis_contract_version must be an integer");
        }
        DateTime::parse_from_rfc3339(
            root.get("generated_at")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("generated_at must be an RFC 3339 string"))?,
        )
        .context("generated_at must be an RFC 3339 timestamp")?;
        serde_json::from_value::<ScopeIdentity>(root["scope"].clone())
            .context("scope does not match the canonical schema-5 contract")?;
        crate::config::validate(&root["config"])
            .context("embedded effective configuration is invalid")?;
        for (index, file) in root["files"]
            .as_array()
            .expect("files checked as array")
            .iter()
            .enumerate()
        {
            serde_json::from_value::<FileAnalysis>(file.clone()).with_context(|| {
                format!("files[{index}] does not match the canonical schema-5 contract")
            })?;
        }
        for (index, folder) in root["folders"]
            .as_array()
            .expect("folders checked as array")
            .iter()
            .enumerate()
        {
            serde_json::from_value::<FolderAnalysis>(folder.clone()).with_context(|| {
                format!("folders[{index}] does not match the canonical schema-5 contract")
            })?;
        }
        serde_json::from_value::<HealthRollup>(root["health"].clone())
            .context("health does not match the canonical schema-5 contract")?;
    }
    Ok(())
}

fn timestamp_slug(generated_at: &str) -> String {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(generated_at) {
        return timestamp
            .with_timezone(&Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string();
    }
    let normalized = generated_at
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let normalized = normalized.trim_matches('-');
    if normalized.is_empty() {
        Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
    } else {
        normalized.to_string()
    }
}

fn temporary_directory(parent: &Path, label: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
    parent.join(format!(".{label}-{}-{sequence}.tmp", std::process::id()))
}

fn unique_run_root(runs_root: &Path, preferred_slug: &str) -> PathBuf {
    let preferred = runs_root.join(preferred_slug);
    if !preferred.exists() {
        return preferred;
    }
    for suffix in 2..10_000 {
        let candidate = runs_root.join(format!("{preferred_slug}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    runs_root.join(format!(
        "{preferred_slug}-{}",
        TEMP_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed)
    ))
}

fn write_bundle_files(
    root: &Path,
    report_json: &str,
    report_yaml: Option<&str>,
    summary: &str,
    health: &str,
    compressed: Option<(&str, &[u8])>,
) -> Result<()> {
    fs::create_dir_all(root)
        .with_context(|| format!("failed to create report directory {}", root.display()))?;
    let mut files = vec![
        ("report.json", report_json),
        ("summary.md", summary),
        ("health.md", health),
    ];
    if let Some(report_yaml) = report_yaml {
        files.push(("report.yaml", report_yaml));
    }
    for (name, content) in files {
        let path = root.join(name);
        fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    }
    if let Some((name, bytes)) = compressed {
        let path = root.join(name);
        fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn replace_latest_from_run(
    latest: &Path,
    run_root: &Path,
    yaml_enabled: bool,
    compressed_name: Option<&str>,
) -> Result<()> {
    let parent = latest.parent().ok_or_else(|| {
        anyhow!(
            "latest report directory has no parent: {}",
            latest.display()
        )
    })?;
    let temporary = temporary_directory(parent, "latest");
    let backup = temporary_directory(parent, "latest-backup");
    fs::create_dir_all(&temporary)?;
    let mut names = vec!["report.json", "summary.md", "health.md"];
    if yaml_enabled {
        names.push("report.yaml");
    }
    if let Some(name) = compressed_name {
        names.push(name);
    }
    for name in names {
        let source = run_root.join(name);
        let target = temporary.join(name);
        if fs::hard_link(&source, &target).is_err() {
            fs::copy(&source, &target)
                .with_context(|| format!("failed to materialize {}", target.display()))?;
        }
    }
    if latest.exists() {
        fs::rename(latest, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, latest) {
        if backup.exists() && !latest.exists() {
            let _ = fs::rename(&backup, latest);
        }
        return Err(error).with_context(|| {
            format!(
                "failed to publish latest report directory {}",
                latest.display()
            )
        });
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}

fn retained_directory_size(path: &Path) -> Result<u64> {
    let mut bytes = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        bytes = bytes.saturating_add(if metadata.is_dir() {
            retained_directory_size(&entry.path())?
        } else {
            metadata.len()
        });
    }
    Ok(bytes)
}

fn enforce_retention(runs_root: &Path, keep: usize, max_bytes: u64) -> Result<()> {
    let mut runs = fs::read_dir(runs_root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| {
            let bytes = retained_directory_size(&entry.path())?;
            Ok((entry, bytes))
        })
        .collect::<Result<Vec<_>>>()?;
    runs.sort_by_key(|(entry, _)| std::cmp::Reverse(entry.file_name()));
    let mut retained_bytes = 0u64;
    for (index, (entry, bytes)) in runs.into_iter().enumerate() {
        if index < keep && retained_bytes.saturating_add(bytes) <= max_bytes {
            retained_bytes = retained_bytes.saturating_add(bytes);
            continue;
        }
        fs::remove_dir_all(entry.path())?;
    }
    Ok(())
}

fn cleanup_abandoned_publication_state(slop_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let latest = slop_root.join("latest");
    let mut backups = Vec::new();
    let Ok(entries) = fs::read_dir(slop_root) else {
        return warnings;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".latest-backup-") && name.ends_with(".tmp") {
            backups.push(entry.path());
        } else if (name.starts_with(".latest-") || name.starts_with(".run-"))
            && name.ends_with(".tmp")
        {
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                warnings.push(format!(
                    "failed to remove abandoned publication temporary {}: {error}",
                    entry.path().display()
                ));
            }
        }
    }
    backups.sort();
    if !latest.exists() {
        if let Some(recovery) = backups.pop() {
            if let Err(error) = fs::rename(&recovery, &latest) {
                warnings.push(format!(
                    "failed to recover latest report from {}: {error}",
                    recovery.display()
                ));
            }
        }
    }
    for backup in backups {
        if let Err(error) = fs::remove_dir_all(&backup) {
            warnings.push(format!(
                "failed to remove abandoned latest backup {}: {error}",
                backup.display()
            ));
        }
    }
    if let Ok(entries) = fs::read_dir(slop_root.join("runs")) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".run-") && name.ends_with(".tmp") {
                if let Err(error) = fs::remove_dir_all(entry.path()) {
                    warnings.push(format!(
                        "failed to remove abandoned run temporary {}: {error}",
                        entry.path().display()
                    ));
                }
            }
        }
    }
    warnings
}

fn write_run_atomically(
    run_root: &Path,
    report_json: &str,
    report_yaml: Option<&str>,
    summary: &str,
    health: &str,
    compressed: Option<(&str, &[u8])>,
) -> Result<()> {
    let parent = run_root
        .parent()
        .ok_or_else(|| anyhow!("run report directory has no parent: {}", run_root.display()))?;
    let temporary = temporary_directory(parent, "run");
    if let Err(error) = write_bundle_files(
        &temporary,
        report_json,
        report_yaml,
        summary,
        health,
        compressed,
    ) {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, run_root) {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error).with_context(|| {
            format!(
                "failed to publish timestamped report directory {}",
                run_root.display()
            )
        });
    }
    Ok(())
}

pub fn write_report_bundle(analysis: &Analysis, health: &HealthRollup) -> Result<FindResult> {
    fs::create_dir_all(&analysis.output_root)
        .with_context(|| format!("failed to create {}", analysis.output_root.display()))?;
    let runs_root = analysis.output_root.join("runs");
    fs::create_dir_all(&runs_root)
        .with_context(|| format!("failed to create {}", runs_root.display()))?;
    let retention = config::pointer_u64(&analysis.config, "/output/retention_runs", 20) as usize;
    let retention_bytes =
        config::pointer_u64(&analysis.config, "/output/retention_bytes", 2_147_483_648);
    let mut warnings = cleanup_abandoned_publication_state(&analysis.output_root);
    let retention_warning =
        enforce_retention(&runs_root, retention.saturating_sub(1), retention_bytes)
            .err()
            .map(|error| format!("old report retention could not be completed: {error:#}"));
    warnings.extend(retention_warning);
    let mut report = assemble_report(analysis, health);
    apply_report_profile(&mut report, &analysis.report_profile);
    if !warnings.is_empty() {
        report["diagnostics"]["warnings"] = json!(warnings);
    }
    let pretty_json = config::pointer_bool(&analysis.config, "/output/pretty_json", false);
    let yaml_enabled = config::pointer_bool(&analysis.config, "/output/yaml", false);
    let mut previous_sizes = (0usize, 0usize);
    for _ in 0..4 {
        let json_bytes = if pretty_json {
            serde_json::to_string_pretty(&report)?.len() + 1
        } else {
            serde_json::to_string(&report)?.len() + 1
        };
        let yaml_bytes = if yaml_enabled {
            serde_yaml::to_string(&report)?.len()
        } else {
            0
        };
        if (json_bytes, yaml_bytes) == previous_sizes {
            break;
        }
        previous_sizes = (json_bytes, yaml_bytes);
        report["diagnostics"]["report_sizes"] = json!({
            "report_json_bytes": json_bytes,
            "report_yaml_bytes": yaml_bytes,
        });
    }
    let report_json = if pretty_json {
        serde_json::to_string_pretty(&report)
    } else {
        serde_json::to_string(&report)
    }
    .context("failed to render report JSON")?
        + "\n";
    let report_yaml = yaml_enabled
        .then(|| serde_yaml::to_string(&report).context("failed to render report YAML"))
        .transpose()?;
    let summary = render_compatibility_summary(&report);
    let health_markdown = render_health_from_report(&report)?;
    let terminal = render_terminal(&report);
    let compressed = compressed_bytes(&analysis.compression, report_json.as_bytes())?;

    let run_root = unique_run_root(&runs_root, &timestamp_slug(&analysis.generated_at));
    write_run_atomically(
        &run_root,
        &report_json,
        report_yaml.as_deref(),
        &summary,
        &health_markdown,
        compressed
            .as_ref()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    )?;
    let latest = analysis.output_root.join("latest");
    replace_latest_from_run(
        &latest,
        &run_root,
        yaml_enabled,
        compressed.as_ref().map(|(name, _)| name.as_str()),
    )?;
    enforce_retention(&runs_root, retention, retention_bytes)?;
    let compressed_report = compressed.map(|(name, _)| latest.join(name));

    Ok(FindResult {
        report,
        report_json: latest.join("report.json"),
        report_yaml: latest.join("report.yaml"),
        summary_md: latest.join("summary.md"),
        health_md: latest.join("health.md"),
        compressed_report,
        terminal,
    })
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn compact_profile_keeps_an_exhaustive_index_and_resolvable_references() {
        let files = (0..300)
            .map(|index| json!({"path": format!("src/{index:03}.rs")}))
            .collect::<Vec<_>>();
        let mut report = json!({
            "files": files.clone(),
            "folders": [],
            "compare_index": {"files": files, "folders": []},
            "ranked_files": [{"path": "src/297.rs"}],
            "action_queue": [{"path": "src/298.rs"}],
            "health": {
                "findings": [{"path": "src/299.rs"}],
                "refactor_candidates": [],
                "watchlist": []
            },
            "overlays": {"organization_health": {
                "relationships": {"temporal_coupling_edges": [{
                    "source_path": "src/298.rs", "target_path": "src/299.rs"
                }]},
                "clusters": {"duplicate_sets": [{
                    "member_paths": ["src/298.rs", "src/299.rs"]
                }]}
            }},
            "summary": {},
            "diagnostics": {},
            "collection_metadata": {}
        });
        apply_report_profile(&mut report, "compact");
        let retained = report["files"]
            .as_array()
            .expect("compact files")
            .iter()
            .filter_map(|record| record["path"].as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(retained.len(), 250);
        for path in ["src/297.rs", "src/298.rs", "src/299.rs"] {
            assert!(retained.contains(path), "missing referenced path {path}");
        }
        assert_eq!(
            report["compare_index"]["files"].as_array().map(Vec::len),
            Some(300)
        );
        assert_eq!(
            report["health"]["findings"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(report["action_queue"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            report["overlays"]["organization_health"]["relationships"]["temporal_coupling_edges"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn standard_bounds_high_cardinality_evidence_while_full_evidence_does_not() {
        let relationships = (0..2_100)
            .map(|index| json!({"id": index}))
            .collect::<Vec<_>>();
        let report = json!({
            "diagnostics": {},
            "overlays": {"organization_health": {"relationships": {
                "temporal_coupling_edges": relationships
            }}}
        });
        let mut standard = report.clone();
        apply_report_profile(&mut standard, "standard");
        let mut full = report;
        apply_report_profile(&mut full, "full_evidence");
        assert_eq!(
            standard
                .pointer("/overlays/organization_health/relationships/temporal_coupling_edges")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2_000)
        );
        assert_eq!(
            full.pointer("/overlays/organization_health/relationships/temporal_coupling_edges")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2_100)
        );
    }
}
