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

fn apply_report_profile(report: &mut Value, profile: &str) {
    report["diagnostics"]["report_profile"] = json!(profile);
    if profile != "compact" {
        return;
    }
    for (key, limit) in [
        ("files", 250),
        ("folders", 250),
        ("ranked_files", 250),
        ("action_queue", 100),
    ] {
        cap_collection(report, key, limit);
    }
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
        "$id": "https://github.com/coreycoto/git-slop/blob/main/schemas/report-5.json",
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
            "action_queue": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["path", "slop_score", "slop_band", "context_band", "tokens", "age_days", "revisions_window", "churn_pressure", "reason_codes", "is_pure_context_hotspot"], "properties":{"path":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"tokens":{"type":"integer"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"churn_pressure":{"type":"number"},"reason_codes":{"type":"array","items":{"type":"string"}},"is_pure_context_hotspot":{"type":"boolean"}}}},
            "ranked_files": {"type": "array", "items": {"$ref": "#/$defs/ranked_file"}},
            "costs": {"type": "object"},
            "overlays": {"type": "object"},
            "health": {"type": "object"},
            "diagnostics": {"type": "object"},
            "collection_metadata": {"type": "object"},
            "evidence_completeness": {"type": "object"}
            ,"terminology": {"type": "object", "required": ["attention_required", "budget_exceeded", "critical", "error"]}
        },
        "$defs": {
            "file": {"type": "object", "additionalProperties": false, "required": ["path", "bytes", "lines", "blank_lines", "code_lines", "comment_lines", "language", "profile", "classification", "analysis_status", "skipped_reason", "symlink_metadata", "has_inline_tests", "tokens", "context_band", "context_pressure", "content_fingerprint", "structural_token_count", "top_structural_terms", "structural_categories", "age_days", "revisions_window", "recency_weighted_commits", "added_window", "deleted_window", "churn_lines_window", "line_churn_window", "token_churn_window", "relative_churn_window", "late_churn_spike", "author_count_window", "author_entropy", "top_author_share", "days_since_non_bot_edit", "recent_maintainer_diversity", "age_pressure", "revision_norm", "relative_churn_norm", "churn_pressure", "slop_score", "slop_band", "reason_codes", "costs", "overlays"], "properties": {"path":{"type":"string"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"blank_lines":{"type":"integer"},"code_lines":{"type":"integer"},"comment_lines":{"type":"integer"},"language":{"type":"string"},"profile":{"type":"string"},"classification":{"type":"string"},"analysis_status":{"type":"string"},"skipped_reason":{"type":["string","null"]},"symlink_metadata":{"type":["object","null"]},"has_inline_tests":{"type":"boolean"},"tokens":{"type":"integer"},"context_band":{"type":"string"},"context_pressure":{"type":"number"},"content_fingerprint":{"type":"string"},"structural_token_count":{"type":"integer"},"top_structural_terms":{"type":"array"},"structural_categories":{"type":"object"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"recency_weighted_commits":{"type":"number"},"added_window":{"type":"integer"},"deleted_window":{"type":"integer"},"churn_lines_window":{"type":"integer"},"line_churn_window":{"type":"integer"},"token_churn_window":{"type":"integer"},"relative_churn_window":{"type":"number"},"late_churn_spike":{"type":"number"},"author_count_window":{"type":"integer"},"author_entropy":{"type":"number"},"top_author_share":{"type":"number"},"days_since_non_bot_edit":{"type":["integer","null"]},"recent_maintainer_diversity":{"type":"integer"},"age_pressure":{"type":"number"},"revision_norm":{"type":"number"},"relative_churn_norm":{"type":"number"},"churn_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"costs":{"type":"object"},"overlays":{"type":"object"}}},
            "folder": {"type": "object", "additionalProperties": false, "required": ["path", "descendant_file_count", "direct_file_count", "bytes", "lines", "tokens", "direct_tokens", "context_band", "health_band", "context_pressure", "slop_score", "slop_band", "reason_codes", "top_file_path", "classification", "costs", "overlays"], "properties":{"path":{"type":"string"},"descendant_file_count":{"type":"integer"},"direct_file_count":{"type":"integer"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"tokens":{"type":"integer"},"direct_tokens":{"type":"integer"},"context_band":{"type":"string"},"health_band":{"type":"string"},"context_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"top_file_path":{"type":"string"},"classification":{"type":"string"},"costs":{"type":"object"},"overlays":{"type":"object"}}},
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
        "slop_score",
        "slop_band",
        "context_band",
        "tokens",
        "age_days",
        "revisions_window",
        "churn_pressure",
        "reason_codes",
        "is_pure_context_hotspot",
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
    Ok(())
}

fn replace_latest_from_run(latest: &Path, run_root: &Path, yaml_enabled: bool) -> Result<()> {
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

fn enforce_retention(runs_root: &Path, keep: usize) -> Result<()> {
    let mut runs = fs::read_dir(runs_root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    runs.sort_by_key(|entry| std::cmp::Reverse(entry.file_name()));
    for entry in runs.into_iter().skip(keep) {
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
) -> Result<()> {
    let parent = run_root
        .parent()
        .ok_or_else(|| anyhow!("run report directory has no parent: {}", run_root.display()))?;
    let temporary = temporary_directory(parent, "run");
    if let Err(error) = write_bundle_files(&temporary, report_json, report_yaml, summary, health) {
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
    let mut warnings = cleanup_abandoned_publication_state(&analysis.output_root);
    let retention_warning = enforce_retention(&runs_root, retention.saturating_sub(1))
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

    let run_root = unique_run_root(&runs_root, &timestamp_slug(&analysis.generated_at));
    write_run_atomically(
        &run_root,
        &report_json,
        report_yaml.as_deref(),
        &summary,
        &health_markdown,
    )?;
    let latest = analysis.output_root.join("latest");
    replace_latest_from_run(&latest, &run_root, yaml_enabled)?;
    let compressed_report = compressed_bytes(&analysis.compression, report_json.as_bytes())?
        .map(|(name, bytes)| -> Result<PathBuf> {
            let run_path = run_root.join(&name);
            fs::write(&run_path, &bytes)
                .with_context(|| format!("failed to write {}", run_path.display()))?;
            let latest_path = latest.join(name);
            fs::write(&latest_path, bytes)
                .with_context(|| format!("failed to write {}", latest_path.display()))?;
            Ok(latest_path)
        })
        .transpose()?;

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
