use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};

mod compare;
mod explain;
mod github;
mod plan;
mod sarif;

pub use compare::{compare_payload, render_compare_text};
pub use explain::{explain_payload, render_explain_text};
pub use github::{health_json_payload, render_github_annotations, write_prompt_pack};
pub use plan::{plan_payload, render_plan_text};
pub use sarif::{render_json, sarif_payload};

const REPORT_SCHEMA_VERSION: i64 = 4;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
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

pub fn failing_records(
    report: &Value,
    fail_on_context_band: Option<&str>,
    fail_on_slop_band: Option<&str>,
) -> Vec<Value> {
    fn context_rank(value: &str) -> i32 {
        match value {
            "compact" => 0,
            "healthy" => 1,
            "warning" => 2,
            "critical" | "refactor_required" => 3,
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
    let mut failures: Vec<Value> = array_at(report, &["files"])
        .iter()
        .filter(|record| {
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
        .cloned()
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
    all_relationships(report, true)
        .into_iter()
        .find(|item| string(item.get("id")) == id)
}

fn cluster_by_id(report: &Value, id: &str) -> Option<Value> {
    all_clusters(report, true)
        .into_iter()
        .find(|item| string(item.get("id")) == id)
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
        "semantic_drift": {"semantic_drift_pressure": maximum("semantic_drift", "semantic_drift_pressure")},
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
            "semantic_drift",
            "semantic_drift",
            "semantic_drift_pressure",
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
        "boundary_note": EXPLAIN_BOUNDARY_NOTE,
    })
}

fn json_scalar_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "None".to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
    }
}
