use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::text::visible_controls;
use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

mod compare;
mod explain;
mod github;
mod plan;
mod sarif;
mod verification;

pub use compare::{compare_payload_with_policy, render_compare_text};
pub use explain::{explain_payload, render_explain_text};
pub use github::{
    PromptPackOptions, health_json_payload, render_github_annotations, write_prompt_pack,
};
pub use plan::{plan_payload, render_plan_text};
pub use sarif::{render_json, sarif_payload};

const REPORT_SCHEMA_VERSION: i64 = 5;
const EXPLAIN_SCHEMA_VERSION: i64 = 2;
const PLAN_SCHEMA_VERSION: i64 = 2;
const COMPARE_SCHEMA_VERSION: i64 = 1;
const SARIF_SCHEMA_VERSION: i64 = 1;
const MAX_SLICE_FILES: usize = 5;

const RELATIONSHIP_KEYS: [&str; 5] = [
    "duplicate_neighborhoods",
    "near_duplicate_neighborhoods",
    "temporal_coupling_edges",
    "lexical_affinity_edges",
    "boundary_leakage_edges",
];
const CLUSTER_KEYS: [&str; 4] = [
    "duplicate_sets",
    "scattered_concepts",
    "boundary_leakage_clusters",
    "consolidation_candidates",
];

pub const EXPLAIN_BOUNDARY_NOTE: &str = "Interpretation boundary: this is structural evidence, not proof that an abstraction, boundary, or refactor is correct.";
pub const PLAN_BOUNDARY_NOTE: &str = "Plan boundary: this is a bounded proposal only. It does not mutate code, GitHub, or detector truth, and it does not guarantee correctness or safety.";
pub const COMPARE_BOUNDARY_NOTE: &str = "Compare boundary: this is a read-only comparison of two existing reports. It does not rerun the detector, imply causality, mutate repo state, or change detector scoring semantics.";
pub const SARIF_BOUNDARY_NOTE: &str = "SARIF export boundary: this is a deterministic projection of existing git-slop report evidence. It does not rerun the detector, upload results, mutate code, or change detector scoring semantics.";

pub(crate) fn inventory_record_has_incomplete_evidence(record: &Value) -> bool {
    match record.get("analysis_status").and_then(Value::as_str) {
        Some("analyzed") => false,
        Some("skipped") => !matches!(
            record.get("skipped_reason").and_then(Value::as_str),
            Some("binary" | "gitlink" | "undecodable")
        ),
        _ => true,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReportReadiness {
    pub analysis_complete: bool,
    pub inventory_complete: bool,
    pub evidence_complete: bool,
    pub comparison_ready: bool,
    pub blockers: Vec<Value>,
}

impl ReportReadiness {
    pub(crate) fn as_json(&self) -> Value {
        json!({
            "analysis_complete": self.analysis_complete,
            "inventory_complete": self.inventory_complete,
            "evidence_complete": self.evidence_complete,
            "comparison_ready": self.comparison_ready,
            "blockers": self.blockers,
        })
    }
}

fn readiness_blocker(code: &str, pointer: &str, message: impl Into<String>) -> Value {
    json!({"code": code, "pointer": pointer, "message": message.into()})
}

/// Canonical readiness evaluation used by check, baseline, compare, prompt
/// packs, and the repository-health Action. Expected non-text inventory
/// records (binary, gitlink, and undecodable) remain complete coverage.
pub(crate) fn evaluate_report_readiness(
    report: &Value,
    require_clean_worktree: bool,
    allow_incomplete_evidence: bool,
) -> ReportReadiness {
    let mut blockers = Vec::new();
    let analysis_status = report
        .pointer("/diagnostics/analysis/analysis_status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let analysis_complete = analysis_status == "complete";
    if !analysis_complete {
        blockers.push(readiness_blocker(
            "analysis_incomplete",
            "/diagnostics/analysis/analysis_status",
            format!("analysis status is {analysis_status}"),
        ));
    }

    if report
        .pointer("/diagnostics/analysis/degraded_omitted_path_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        > 0
    {
        blockers.push(readiness_blocker(
            "inventory_paths_omitted",
            "/diagnostics/analysis/degraded_omitted_path_count",
            "selected tracked paths were omitted by resource degradation",
        ));
    }

    let profile = report
        .pointer("/analyzer/report_profile")
        .and_then(Value::as_str);
    let has_compare_index = report
        .pointer("/compare_index/files")
        .and_then(Value::as_array)
        .is_some();
    let has_policy_index = report
        .pointer("/policy_index/files")
        .and_then(Value::as_array)
        .is_some();
    if profile == Some("compact") && !has_compare_index {
        blockers.push(readiness_blocker(
            "comparison_index_missing",
            "/compare_index/files",
            "compact report is missing its exhaustive comparison index",
        ));
    }
    if profile == Some("compact") && !has_policy_index {
        blockers.push(readiness_blocker(
            "policy_index_missing",
            "/policy_index/files",
            "compact report is not enforcement-complete without its exhaustive policy index",
        ));
    }
    for collection in ["files", "folders"] {
        let pointer = if has_compare_index {
            format!("/collection_metadata/compare_index/{collection}")
        } else {
            format!("/collection_metadata/{collection}")
        };
        match report.pointer(&pointer) {
            Some(metadata) => {
                let total = metadata.get("total").and_then(Value::as_u64);
                let returned = metadata.get("returned").and_then(Value::as_u64);
                let truncated = metadata
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if truncated || total.is_none() || returned.is_none() || total != returned {
                    blockers.push(readiness_blocker(
                        "canonical_collection_incomplete",
                        &pointer,
                        format!("canonical {collection} collection is incomplete"),
                    ));
                }
            }
            None => blockers.push(readiness_blocker(
                "collection_metadata_missing",
                &pointer,
                format!("canonical {collection} collection metadata is missing"),
            )),
        }
    }
    if has_policy_index {
        for collection in ["files", "folders"] {
            let pointer = format!("/collection_metadata/policy_index/{collection}");
            match report.pointer(&pointer) {
                Some(metadata) => {
                    let total = metadata.get("total").and_then(Value::as_u64);
                    let returned = metadata.get("returned").and_then(Value::as_u64);
                    let truncated = metadata
                        .get("truncated")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if truncated || total.is_none() || returned.is_none() || total != returned {
                        blockers.push(readiness_blocker(
                            "policy_index_incomplete",
                            &pointer,
                            format!("canonical {collection} policy index is incomplete"),
                        ));
                    }
                }
                None => blockers.push(readiness_blocker(
                    "collection_metadata_missing",
                    &pointer,
                    format!("canonical {collection} policy index metadata is missing"),
                )),
            }
        }
    }

    let inventory = report
        .pointer("/compare_index/files")
        .or_else(|| report.get("files"))
        .and_then(Value::as_array);
    let incomplete_records = inventory
        .into_iter()
        .flatten()
        .filter(|record| inventory_record_has_incomplete_evidence(record))
        .count();
    let compare_records = report
        .pointer("/compare_index/files")
        .and_then(Value::as_array)
        .map(|records| {
            records
                .iter()
                .filter_map(|record| Some((record.get("path")?.as_str()?, record)))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let inconsistent_records = report
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| {
            let Some(path) = record.get("path").and_then(Value::as_str) else {
                return true;
            };
            let Some(compare) = compare_records.get(path) else {
                return true;
            };
            [
                "content_sha256",
                "content_fingerprint",
                "analysis_status",
                "skipped_reason",
            ]
            .iter()
            .any(|field| record.get(*field) != compare.get(*field))
        })
        .count();
    let inventory_complete = incomplete_records == 0 && inconsistent_records == 0;
    if !inventory_complete && !allow_incomplete_evidence {
        blockers.push(readiness_blocker(
            "inventory_evidence_incomplete",
            if has_compare_index {
                "/compare_index/files"
            } else {
                "/files"
            },
            format!("{incomplete_records} canonical inventory record(s) are incomplete"),
        ));
        if inconsistent_records > 0 {
            blockers.push(readiness_blocker(
                "comparison_index_inconsistent",
                "/compare_index/files",
                format!("{inconsistent_records} public file record(s) disagree with the comparison index"),
            ));
        }
    }

    let incomplete_evidence = report
        .get("evidence_completeness")
        .and_then(Value::as_object)
        .map(|evidence| {
            evidence
                .iter()
                .filter_map(|(field, value)| {
                    value
                        .as_str()
                        .filter(|status| status.starts_with("incomplete_"))
                        .map(|status| (field.clone(), status.to_string()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let evidence_complete = incomplete_evidence.is_empty();
    if !evidence_complete && !allow_incomplete_evidence {
        for (field, status) in incomplete_evidence {
            blockers.push(readiness_blocker(
                "evidence_incomplete",
                &format!("/evidence_completeness/{field}"),
                format!("evidence status is {status}"),
            ));
        }
    }

    if require_clean_worktree
        && report
            .pointer("/repo/worktree_clean")
            .and_then(Value::as_bool)
            == Some(false)
    {
        blockers.push(readiness_blocker(
            "dirty_worktree_baseline",
            "/repo/worktree_clean",
            "baseline was captured from a dirty worktree",
        ));
    }

    let comparison_ready = blockers.is_empty();
    ReportReadiness {
        analysis_complete,
        inventory_complete,
        evidence_complete,
        comparison_ready,
        blockers,
    }
}

#[derive(Debug, Clone)]
pub enum ExplainSelector {
    Path(String),
    Cluster(String),
    Relationship(String),
    Top(usize),
}

#[derive(Debug, Clone)]
pub enum PlanSelector {
    Path(String),
    Cluster(String),
    Relationship(String),
}

fn array_at<'a>(value: &'a Value, path: &[&str]) -> &'a [Value] {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return &[];
        };
        current = next;
    }
    current.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn string_or(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn number(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToOwned::to_owned)
}

fn integer(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
                .or_else(|| value.as_f64().map(|value| value as i64))
        })
        .unwrap_or(0)
}

fn usize_value(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn cmp_f64_desc(left: f64, right: f64) -> Ordering {
    right.partial_cmp(&left).unwrap_or(Ordering::Equal)
}

fn evidence_then_id(left: &Value, right: &Value) -> Ordering {
    cmp_f64_desc(
        number(left.get("evidence_score")),
        number(right.get("evidence_score")),
    )
    .then_with(|| string(left.get("id")).cmp(&string(right.get("id"))))
}

fn normalized_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "." {
        ".".to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

fn path_matches_folder(path: &str, folder: &str) -> bool {
    folder == "." || path.starts_with(&format!("{}/", folder.trim_end_matches('/')))
}

fn report_schema(report: &Value) -> i64 {
    integer(report.get("schema_version"))
}

fn require_report_schema(report: &Value, command: &str) -> Result<()> {
    if report_schema(report) != REPORT_SCHEMA_VERSION {
        bail!("git slop {command} requires report schema {REPORT_SCHEMA_VERSION}.");
    }
    Ok(())
}

fn relationship_sections(report: &Value, canonical_first: bool) -> &Value {
    let top_level = report.get("relationships").filter(|value| {
        value
            .as_object()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    });
    let canonical = value_at(
        report,
        &["overlays", "organization_health", "relationships"],
    )
    .filter(|value| {
        value
            .as_object()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    });
    if canonical_first {
        canonical.or(top_level).unwrap_or(&Value::Null)
    } else {
        top_level.or(canonical).unwrap_or(&Value::Null)
    }
}

fn cluster_sections(report: &Value, canonical_first: bool) -> &Value {
    let top_level = report.get("clusters").filter(|value| {
        value
            .as_object()
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    });
    let canonical =
        value_at(report, &["overlays", "organization_health", "clusters"]).filter(|value| {
            value
                .as_object()
                .map(|value| !value.is_empty())
                .unwrap_or(false)
        });
    if canonical_first {
        canonical.or(top_level).unwrap_or(&Value::Null)
    } else {
        top_level.or(canonical).unwrap_or(&Value::Null)
    }
}

fn all_relationships(report: &Value, canonical_first: bool) -> Vec<Value> {
    let sections = relationship_sections(report, canonical_first);
    let mut result = Vec::new();
    for key in RELATIONSHIP_KEYS {
        result.extend(array_at(sections, &[key]).iter().cloned());
    }
    result.sort_by(evidence_then_id);
    dedupe_by_id(result)
}

fn all_clusters(report: &Value, canonical_first: bool) -> Vec<Value> {
    let sections = cluster_sections(report, canonical_first);
    let mut result = Vec::new();
    for key in CLUSTER_KEYS {
        result.extend(array_at(sections, &[key]).iter().cloned());
    }
    result.sort_by(evidence_then_id);
    // A consolidation-candidate mirror may intentionally reuse its source
    // cluster ID. Preserve memberships across cluster kinds; ID-only lookup
    // remains deterministic because section order is stable.
    result
}

fn dedupe_by_id(items: Vec<Value>) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|item| {
            let id = string(item.get("id"));
            !id.is_empty() && seen.insert(id)
        })
        .collect()
}

fn matching_relationships(report: &Value, target: &str, folder: bool) -> Vec<Value> {
    let mut result: Vec<Value> = all_relationships(report, false)
        .into_iter()
        .filter(|item| {
            let source = string(item.get("source_path"));
            let target_path = string(item.get("target_path"));
            if folder {
                path_matches_folder(&source, target) || path_matches_folder(&target_path, target)
            } else {
                source == target || target_path == target
            }
        })
        .collect();
    result.sort_by(evidence_then_id);
    result
}

fn matching_clusters(report: &Value, target: &str, folder: bool) -> Vec<Value> {
    let mut result: Vec<Value> = all_clusters(report, false)
        .into_iter()
        .filter(|item| {
            string_array(item.get("member_paths")).iter().any(|member| {
                if folder {
                    path_matches_folder(member, target)
                } else {
                    member == target
                }
            })
        })
        .collect();
    result.sort_by(evidence_then_id);
    result
}

fn find_record(report: &Value, target: &str) -> Option<(Value, bool)> {
    let target = normalized_path(target);
    for record in array_at(report, &["files"]) {
        if string(record.get("path")) == target {
            return Some((record.clone(), true));
        }
    }
    for record in array_at(report, &["folders"]) {
        if string(record.get("path")) == target {
            return Some((record.clone(), false));
        }
    }
    None
}

pub fn show_payload(report: &Value, target: &str) -> Option<Value> {
    let target = normalized_path(target);
    let (record, is_file) = find_record(report, &target)?;
    let mut payload = record.as_object()?.clone();
    let overlays = payload.get("overlays").cloned().unwrap_or(Value::Null);
    payload.insert(
        "record_type".to_string(),
        Value::String(if is_file { "file" } else { "folder" }.to_string()),
    );
    payload.insert(
        "organization_health".to_string(),
        overlays
            .get("organization_health")
            .cloned()
            .unwrap_or(Value::Null),
    );
    payload.insert(
        "strongest_relationships".to_string(),
        Value::Array(
            matching_relationships(report, &target, !is_file)
                .into_iter()
                .take(10)
                .collect(),
        ),
    );
    payload.insert(
        "cluster_memberships".to_string(),
        Value::Array(
            matching_clusters(report, &target, !is_file)
                .into_iter()
                .take(10)
                .collect(),
        ),
    );
    Some(Value::Object(payload))
}

pub fn render_show_text(payload: &Value) -> String {
    let kind = string_or(payload.get("record_type"), "record");
    let path = visible_controls(&string(payload.get("path")));
    let mut lines = vec![format!(
        "{}: {}",
        if kind == "file" { "File" } else { "Folder" },
        path
    )];
    lines.push(format!(
        "tokens={} context={} slop={} score={:.1}",
        integer(payload.get("tokens")),
        string_or(payload.get("context_band"), "unknown"),
        string_or(payload.get("slop_band"), "unknown"),
        number(payload.get("slop_score")),
    ));
    let reasons = string_array(payload.get("reason_codes"));
    if !reasons.is_empty() {
        lines.push(format!("reasons: {}", reasons.join(", ")));
    }
    let relationships = array_at(payload, &["strongest_relationships"]);
    if !relationships.is_empty() {
        lines.push("relationships:".to_string());
        for item in relationships.iter().take(5) {
            lines.push(format!(
                "- {} ↔ {} kind={} confidence={} lower={:.3} support={} evidence={:.3} id={}",
                visible_controls(&string(item.get("source_path"))),
                visible_controls(&string(item.get("target_path"))),
                string(item.get("kind")),
                string_or(item.get("confidence"), "unknown"),
                number(
                    item.get("evidence_lower_bound")
                        .or_else(|| item.get("confidence_lower_bound")),
                ),
                integer(item.get("support_count")),
                number(item.get("evidence_score")),
                visible_controls(&string(item.get("id"))),
            ));
        }
    }
    lines.push(format!("next: git slop explain --path {}", path));
    lines.join("\n") + "\n"
}

pub fn failing_records_in(
    report: &Value,
    fail_on_context_band: Option<&str>,
    fail_on_slop_band: Option<&str>,
    include_folders: bool,
) -> Vec<Value> {
    fn context_rank(value: &str) -> i32 {
        match value {
            "compact" => 0,
            "healthy" => 1,
            "warning" => 2,
            "critical" | "refactor_required" | "budget_exceeded" => 3,
            _ => -1,
        }
    }
    fn slop_rank(value: &str) -> i32 {
        match value {
            "low" => 0,
            "moderate" => 1,
            "high" => 2,
            "critical" => 3,
            _ => -1,
        }
    }
    let collections: &[&str] = if include_folders {
        &["files", "folders"]
    } else {
        &["files"]
    };
    let indexed = report.get("policy_index").and_then(Value::as_object);
    let mut failures: Vec<Value> = collections
        .iter()
        .flat_map(|collection| {
            let records: &[Value] = indexed
                .and_then(|index| index.get(*collection))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_else(|| array_at(report, &[*collection]));
            records.iter().map(move |record| (*collection, record))
        })
        .filter(|record| {
            let record = record.1;
            if matches!(
                string(record.get("classification")).as_str(),
                "generated" | "vendored" | "snapshot" | "fixture" | "migration_fixture"
            ) {
                return false;
            }
            let context_failed = fail_on_context_band
                .map(|threshold| {
                    context_rank(&string(record.get("context_band"))) >= context_rank(threshold)
                })
                .unwrap_or(false);
            let slop_failed = fail_on_slop_band
                .map(|threshold| {
                    slop_rank(&string(record.get("slop_band"))) >= slop_rank(threshold)
                })
                .unwrap_or(false);
            context_failed || slop_failed
        })
        .map(|(collection, record)| {
            let mut record = record.clone();
            record["record_type"] = json!(if collection == "files" {
                "file"
            } else {
                "folder"
            });
            record
        })
        .collect();
    failures.sort_by(|left, right| {
        cmp_f64_desc(
            number(left.get("slop_score")),
            number(right.get("slop_score")),
        )
        .then_with(|| usize_value(right.get("tokens")).cmp(&usize_value(left.get("tokens"))))
        .then_with(|| string(left.get("path")).cmp(&string(right.get("path"))))
    });
    failures
}

fn record_summary(record: Option<&Value>) -> Value {
    let Some(record) = record else {
        return Value::Null;
    };
    let mut result = Map::new();
    for key in [
        "path",
        "slop_score",
        "slop_band",
        "context_band",
        "reason_codes",
    ] {
        result.insert(
            key.to_string(),
            record.get(key).cloned().unwrap_or_else(|| match key {
                "reason_codes" => Value::Array(Vec::new()),
                _ => Value::Null,
            }),
        );
    }
    if let Some(costs) = record.get("costs") {
        result.insert("costs".to_string(), costs.clone());
    }
    if let Some(overlays) = record.get("overlays") {
        result.insert("overlays".to_string(), overlays.clone());
    }
    Value::Object(result)
}

fn resolved_record(report: &Value, path: &str) -> Option<Value> {
    show_payload(report, path)
}

fn relationship_by_id(report: &Value, id: &str) -> Option<Value> {
    unique_id_match(all_relationships(report, true), id)
}

fn cluster_by_id(report: &Value, id: &str) -> Option<Value> {
    unique_id_match(all_clusters(report, true), id)
}

fn unique_id_match(items: Vec<Value>, selector: &str) -> Option<Value> {
    if let Some(exact) = items.iter().find(|item| string(item.get("id")) == selector) {
        return Some(exact.clone());
    }
    let mut prefixes = items
        .into_iter()
        .filter(|item| string(item.get("id")).starts_with(selector));
    let selected = prefixes.next()?;
    prefixes.next().is_none().then_some(selected)
}

fn descendant_records(report: &Value, folder: &str) -> Vec<Value> {
    let mut records: Vec<Value> = array_at(report, &["files"])
        .iter()
        .filter(|record| path_matches_folder(&string(record.get("path")), folder))
        .cloned()
        .collect();
    records.sort_by(|left, right| {
        cmp_f64_desc(
            number(left.get("slop_score")),
            number(right.get("slop_score")),
        )
        .then_with(|| string(left.get("path")).cmp(&string(right.get("path"))))
    });
    records
}

fn descendant_hotspots(report: &Value, folder: &str, limit: usize) -> Vec<Value> {
    array_at(report, &["action_queue"])
        .iter()
        .filter(|record| path_matches_folder(&string(record.get("path")), folder))
        .take(limit)
        .cloned()
        .collect()
}

fn overlay_value(record: &Value, overlay: &str, key: &str) -> f64 {
    number(value_at(record, &["overlays", overlay, key]))
}

fn descendant_overlay_maxima(records: &[Value]) -> Value {
    let maximum = |overlay: &str, key: &str| {
        records
            .iter()
            .map(|record| overlay_value(record, overlay, key))
            .fold(0.0, f64::max)
    };
    json!({
        "organization_health": {
            "duplication_pressure": maximum("organization_health", "duplication_pressure"),
            "diffusion_pressure": maximum("organization_health", "diffusion_pressure"),
            "coupling_pressure": maximum("organization_health", "coupling_pressure"),
            "boundary_pressure": maximum("organization_health", "boundary_pressure"),
        },
        "verification": {"verification_gap": maximum("verification", "verification_gap")},
        "navigation": {"navigation_pressure": maximum("navigation", "navigation_pressure")},
        "blast_radius": {"blast_radius_pressure": maximum("blast_radius", "blast_radius_pressure")},
        "stewardship": {"stewardship_pressure": maximum("stewardship", "stewardship_pressure")},
        "concept_dispersion": {"concept_dispersion_pressure": maximum("concept_dispersion", "concept_dispersion_pressure")},
    })
}

fn relationship_focus(item: &Value, anchors: &[String], folder: Option<&str>) -> (usize, usize) {
    let endpoints = [
        string(item.get("source_path")),
        string(item.get("target_path")),
    ];
    let anchor_matches = endpoints
        .iter()
        .filter(|path| anchors.contains(path))
        .count();
    let folder_matches = folder
        .map(|folder| {
            endpoints
                .iter()
                .filter(|path| path_matches_folder(path, folder))
                .count()
        })
        .unwrap_or(anchor_matches);
    (folder_matches, anchor_matches)
}

fn rank_relationships(items: Vec<Value>, anchors: &[String], folder: Option<&str>) -> Vec<Value> {
    let mut ranked: Vec<Value> = dedupe_by_id(items)
        .into_iter()
        .filter(|item| folder.is_none() || relationship_focus(item, anchors, folder).0 > 0)
        .collect();
    ranked.sort_by(|left, right| {
        let left_focus = relationship_focus(left, anchors, folder);
        let right_focus = relationship_focus(right, anchors, folder);
        right_focus
            .0
            .cmp(&left_focus.0)
            .then_with(|| right_focus.1.cmp(&left_focus.1))
            .then_with(|| {
                cmp_f64_desc(
                    number(left.get("evidence_score")),
                    number(right.get("evidence_score")),
                )
            })
            .then_with(|| string(left.get("id")).cmp(&string(right.get("id"))))
    });
    ranked
}

fn cluster_focus(item: &Value, anchors: &[String], folder: Option<&str>) -> (usize, usize) {
    let members = string_array(item.get("member_paths"));
    let anchor_matches = members.iter().filter(|path| anchors.contains(path)).count();
    let folder_matches = folder
        .map(|folder| {
            members
                .iter()
                .filter(|path| path_matches_folder(path, folder))
                .count()
        })
        .unwrap_or(anchor_matches);
    (folder_matches, anchor_matches)
}

fn rank_clusters(items: Vec<Value>, anchors: &[String], folder: Option<&str>) -> Vec<Value> {
    let mut ranked: Vec<Value> = dedupe_by_id(items)
        .into_iter()
        .filter(|item| folder.is_none() || cluster_focus(item, anchors, folder).0 > 0)
        .collect();
    ranked.sort_by(|left, right| {
        let left_focus = cluster_focus(left, anchors, folder);
        let right_focus = cluster_focus(right, anchors, folder);
        let left_count = usize_value(left.get("member_count"))
            .max(string_array(left.get("member_paths")).len())
            .max(1);
        let right_count = usize_value(right.get("member_count"))
            .max(string_array(right.get("member_paths")).len())
            .max(1);
        let left_density = left_focus.0 as f64 / left_count as f64;
        let right_density = right_focus.0 as f64 / right_count as f64;
        cmp_f64_desc(left_density, right_density)
            .then_with(|| right_focus.1.cmp(&left_focus.1))
            .then_with(|| left_count.cmp(&right_count))
            .then_with(|| right_focus.0.cmp(&left_focus.0))
            .then_with(|| {
                string_array(left.get("top_level_roots"))
                    .len()
                    .cmp(&string_array(right.get("top_level_roots")).len())
            })
            .then_with(|| {
                cmp_f64_desc(
                    number(left.get("evidence_score")),
                    number(right.get("evidence_score")),
                )
            })
            .then_with(|| string(left.get("id")).cmp(&string(right.get("id"))))
    });
    ranked
}

fn shared_clusters_for_relationship(report: &Value, relationship: &Value) -> Vec<Value> {
    let source = string(relationship.get("source_path"));
    let target = string(relationship.get("target_path"));
    let anchors = vec![source.clone(), target.clone()];
    rank_clusters(
        all_clusters(report, true)
            .into_iter()
            .filter(|cluster| {
                let members = string_array(cluster.get("member_paths"));
                members.contains(&source) && members.contains(&target)
            })
            .collect(),
        &anchors,
        None,
    )
}

fn strongest_pressures(overlays: Option<&Value>, limit: usize) -> Vec<(String, f64)> {
    let Some(overlays) = overlays.and_then(Value::as_object) else {
        return Vec::new();
    };
    let specs = [
        (
            "organization.duplication",
            "organization_health",
            "duplication_pressure",
        ),
        (
            "organization.diffusion",
            "organization_health",
            "diffusion_pressure",
        ),
        (
            "organization.coupling",
            "organization_health",
            "coupling_pressure",
        ),
        (
            "organization.boundary",
            "organization_health",
            "boundary_pressure",
        ),
        ("verification", "verification", "verification_gap"),
        ("navigation", "navigation", "navigation_pressure"),
        ("blast_radius", "blast_radius", "blast_radius_pressure"),
        ("stewardship", "stewardship", "stewardship_pressure"),
        (
            "concept_dispersion",
            "concept_dispersion",
            "concept_dispersion_pressure",
        ),
    ];
    let mut values: Vec<(String, f64)> = specs
        .into_iter()
        .filter_map(|(label, family, key)| {
            let value = overlays
                .get(family)
                .and_then(|value| value.as_object())
                .and_then(|value| value.get(key))
                .and_then(Value::as_f64)?;
            Some((label.to_string(), value))
        })
        .filter(|(_, value)| *value > 0.0)
        .collect();
    values.sort_by(|left, right| cmp_f64_desc(left.1, right.1).then_with(|| left.0.cmp(&right.0)));
    values.truncate(limit);
    values
}

fn cost_evidence_summary(costs: Option<&Value>) -> Vec<String> {
    let costs = costs.unwrap_or(&Value::Null);
    let load = number(value_at(costs, &["load", "load_pressure"]));
    let tokens = integer(value_at(costs, &["load", "file_token_count"]));
    let volatility = number(value_at(costs, &["volatility", "volatility_pressure"]));
    let commits = integer(value_at(costs, &["volatility", "commit_count_window"]));
    let coordination = number(value_at(costs, &["coordination", "coordination_pressure"]));
    let degree = integer(value_at(costs, &["coordination", "cochange_degree"]));
    let mut values = vec![
        (
            load,
            format!("load pressure {load:.3} from {tokens} file tokens"),
        ),
        (
            volatility,
            format!("volatility pressure {volatility:.3} from {commits} commits"),
        ),
        (
            coordination,
            format!("coordination pressure {coordination:.3} from degree {degree}"),
        ),
    ];
    values.sort_by(|left, right| cmp_f64_desc(left.0, right.0).then_with(|| left.1.cmp(&right.1)));
    values.into_iter().map(|(_, text)| text).take(3).collect()
}

fn evidence_summary(payload: &Value, mode: &str) -> Value {
    let relationships: Vec<String> = array_at(payload, &["supporting_relationships"])
        .iter()
        .take(5)
        .map(|item| string(item.get("id")))
        .collect();
    let clusters: Vec<String> = array_at(payload, &["supporting_clusters"])
        .iter()
        .take(5)
        .map(|item| string(item.get("id")))
        .collect();
    let overlay_summary = payload.get("overlay_summary");
    json!({
        "detector_cost": cost_evidence_summary(payload.get("cost_summary")),
        "strongest_overlays": strongest_pressures(overlay_summary, 3)
            .into_iter()
            .map(|(label, value)| format!("{label} pressure {value:.3}"))
            .collect::<Vec<_>>(),
        "supporting_evidence": {
            "relationship_ids": relationships,
            "cluster_ids": clusters,
        },
        "interpretation": format!("{mode} explanation is based on detector report evidence only; it does not prove correctness or require a refactor."),
    })
}

fn base_explain_payload(report: &Value, selector: Value, target: Value) -> Value {
    json!({
        "schema_version": EXPLAIN_SCHEMA_VERSION,
        "report_schema_version": report_schema(report),
        "command": "explain",
        "selector": selector,
        "target": target,
        "report_context": explain_report_context(report),
        "boundary_note": EXPLAIN_BOUNDARY_NOTE,
    })
}

fn explain_report_context(report: &Value) -> Value {
    let report_digest = serde_json::to_vec(report)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .unwrap_or_default();
    let completeness = report
        .get("evidence_completeness")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let incomplete = completeness.as_object().is_some_and(|values| {
        values.values().any(|value| {
            value.as_str().is_some_and(|status| {
                status.contains("incomplete") || matches!(status, "bounded" | "low_support")
            })
        })
    });
    json!({
        "report_digest": report_digest,
        "content_digest": report.pointer("/repo/analyzed_content_digest").cloned().unwrap_or(Value::Null),
        "head_sha": report.pointer("/repo/head_sha").cloned().unwrap_or(Value::Null),
        "generated_at": report.get("generated_at").cloned().unwrap_or(Value::Null),
        "analyzed_revision_at": report.get("analyzed_revision_at").cloned().unwrap_or(Value::Null),
        "analyzer": report.get("analyzer").cloned().unwrap_or(Value::Null),
        "config_digests": {
            "analysis": report.pointer("/analyzer/analysis_config_digest").cloned().unwrap_or(Value::Null),
            "evidence": report.pointer("/analyzer/evidence_config_digest").cloned().unwrap_or(Value::Null),
            "policy": report.pointer("/analyzer/policy_config_digest").cloned().unwrap_or(Value::Null),
            "presentation": report.pointer("/analyzer/presentation_config_digest").cloned().unwrap_or(Value::Null)
        },
        "evidence_completeness": completeness,
        "evidence_characteristics": {
            "stable_cost_models": ["load", "volatility", "coordination"],
            "experimental_overlays": ["organization_health", "verification", "navigation", "blast_radius", "stewardship", "concept_dispersion"],
            "incomplete": incomplete,
            "repository_relative": true,
            "saturation_suppressed": report.pointer("/diagnostics/suppressed_saturated_overlays").cloned().unwrap_or_else(|| json!([]))
        },
        "collection_metadata": report.get("collection_metadata").cloned().unwrap_or_else(|| json!({}))
    })
}

fn json_scalar_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "None".to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
    }
}
