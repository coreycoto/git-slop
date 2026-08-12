use super::*;
use sha2::{Digest, Sha256};

fn sarif_level(record: &Value) -> &'static str {
    let slop_band = string(record.get("slop_band"));
    let context_band = string(record.get("context_band"));
    if slop_band == "critical"
        || matches!(
            context_band.as_str(),
            "critical" | "refactor_required" | "budget_exceeded"
        )
    {
        "error"
    } else if matches!(slop_band.as_str(), "high" | "moderate") || context_band == "warning" {
        "warning"
    } else {
        "note"
    }
}

fn sarif_record(report: &Value, queue_item: &Value) -> Value {
    let path = string(queue_item.get("path"));
    let mut record = find_record(report, &path)
        .map(|(record, _)| record)
        .unwrap_or_else(|| queue_item.clone());
    let record_object = record.as_object_mut().expect("record object");
    for key in [
        "classification",
        "remediation_kind",
        "slop_score",
        "slop_band",
        "context_band",
        "reason_codes",
    ] {
        if !record_object.contains_key(key) {
            record_object.insert(
                key.to_string(),
                queue_item.get(key).cloned().unwrap_or_else(|| {
                    if key == "reason_codes" {
                        json!([])
                    } else {
                        Value::Null
                    }
                }),
            );
        }
    }
    record
}

fn sarif_result(record: &Value, rank: usize) -> Value {
    let path = string(record.get("path"));
    let reasons = string_array(record.get("reason_codes"));
    let reasons_text = if reasons.is_empty() {
        "no reason codes".to_string()
    } else {
        reasons.join(", ")
    };
    let overlays: Map<String, Value> = strongest_pressures(record.get("overlays"), 8)
        .into_iter()
        .map(|(label, value)| (label, json!(round6(value))))
        .collect();
    let context_finding = matches!(
        string(record.get("context_band")).as_str(),
        "warning" | "critical" | "refactor_required" | "budget_exceeded"
    );
    let rule_id = if context_finding {
        "git-slop.context-budget"
    } else {
        "git-slop.maintenance-pressure"
    };
    let fingerprint = hex::encode(Sha256::digest(format!("{rule_id}\0{path}").as_bytes()));
    let mut result = json!({
        "ruleId": rule_id,
        "ruleIndex": if context_finding { 0 } else { 1 },
        "level": sarif_level(record),
        "message": {
            "text": format!(
                "{path} is ranked {} with slop_score {} and context {} ({reasons_text}).",
                json_scalar_text(record.get("slop_band")),
                json_scalar_text(record.get("slop_score")),
                json_scalar_text(record.get("context_band")),
            ),
        },
        "locations": [{
            "physicalLocation": {
                "artifactLocation": {"uri": path},
            },
        }],
        "partialFingerprints": {"gitSlopFinding/v1": fingerprint},
        "properties": {
            "git_slop": {
                "rank": rank,
                "slop_score": record.get("slop_score").cloned().unwrap_or(Value::Null),
                "slop_band": record.get("slop_band").cloned().unwrap_or(Value::Null),
                "context_band": record.get("context_band").cloned().unwrap_or(Value::Null),
                "classification": record.get("classification").cloned().unwrap_or(Value::Null),
                "remediation_kind": record.get("remediation_kind").cloned().unwrap_or(Value::Null),
                "reason_codes": record.get("reason_codes").cloned().unwrap_or_else(|| json!([])),
                "costs": record.get("costs").cloned().unwrap_or_else(|| json!({})),
                "strongest_overlays": Value::Object(overlays),
                "evidence_boundary": "Hotspot cost and overlay evidence are preserved as separate properties; SARIF export does not rescore the finding.",
            },
        },
    });
    if let Some(state) = record.get("baseline_state").and_then(Value::as_str) {
        result["baselineState"] = json!(state);
    }
    result
}

pub fn sarif_payload(
    report: &Value,
    report_path: Option<&str>,
    top: Option<usize>,
    scope: &str,
) -> Result<Value> {
    require_report_schema(report, "sarif")?;
    if top == Some(0) {
        bail!("--top must be greater than zero.");
    }
    let policy_records;
    let queue = if scope == "policy" {
        let context = report
            .pointer("/config/check/fail_on_context_band")
            .and_then(Value::as_str);
        let slop = report
            .pointer("/config/check/fail_on_slop_band")
            .and_then(Value::as_str);
        policy_records = super::failing_records_in(report, context, slop, false);
        policy_records.as_slice()
    } else if scope == "action-queue" {
        array_at(report, &["action_queue"])
    } else {
        bail!("SARIF scope must be policy or action-queue.");
    };
    let take = top.unwrap_or(queue.len());
    let results: Vec<Value> = queue
        .iter()
        .take(take)
        .enumerate()
        .filter_map(|(index, item)| {
            item.get("path")
                .and_then(Value::as_str)
                .map(|_| sarif_result(&sarif_record(report, item), index + 1))
        })
        .collect();
    let returned = results.len();
    let help_uri = format!(
        "https://github.com/coreycoto/git-slop/blob/v{}/docs/scoring-model.md",
        crate::VERSION
    );
    let repo = report.get("repo").unwrap_or(&Value::Null);
    let repository_uri = ["remote_url", "git_remote_url", "repo_name"]
        .into_iter()
        .find_map(|key| {
            repo.get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        })
        .map_or(Value::Null, |value| json!(value));
    let revision_id = ["head_sha", "head_commit"]
        .into_iter()
        .find_map(|key| {
            repo.get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        })
        .map_or(Value::Null, |value| json!(value));
    Ok(json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "git-slop",
                    "informationUri": "https://github.com/coreycoto/git-slop",
                    "rules": [{
                        "id": "git-slop.context-budget",
                        "name": "Git Slop context budget",
                        "shortDescription": {"text": "File meets a configured context-cost threshold."},
                        "fullDescription": {"text": "A deterministic context-cost finding from Git Slop."},
                        "helpUri": help_uri,
                        "help": {"text": "Review the git-slop report, explain output, or plan output for supporting evidence before deciding whether maintenance work is appropriate."},
                        "properties": {
                            "precision": "medium",
                            "tags": ["maintainability", "context-cost", "git-slop"],
                        },
                    }, {
                        "id": "git-slop.maintenance-pressure",
                        "name": "Git Slop maintenance pressure",
                        "shortDescription": {"text": "File meets a configured maintenance-pressure threshold."},
                        "fullDescription": {"text": "A deterministic maintenance-pressure finding from Git Slop."},
                        "helpUri": help_uri,
                        "properties": {"precision": "medium", "tags": ["maintainability", "git-slop"]}
                    }],
                },
            },
            "automationDetails": {"id": "git-slop/sarif"},
            "versionControlProvenance": [{
                "repositoryUri": repository_uri,
                "revisionId": revision_id,
            }],
            "invocations": [{
                "executionSuccessful": true,
                "properties": {
                    "git_slop": {
                        "schema_version": SARIF_SCHEMA_VERSION,
                        "report_schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
                        "report_path": report_path,
                        "analyzer": report.get("analyzer").cloned().unwrap_or_else(|| json!({})),
                        "boundary_note": SARIF_BOUNDARY_NOTE,
                        "scope": scope,
                        "collection": {
                            "total": queue.len(),
                            "returned": returned,
                            "limit": top,
                            "truncated": returned < queue.len()
                        },
                    },
                },
            }],
            "results": results,
            "properties": {
                "git_slop": {
                    "summary": report.get("summary").cloned().unwrap_or_else(|| json!({})),
                    "stats": report.get("stats").cloned().unwrap_or_else(|| json!({})),
                    "boundary_note": SARIF_BOUNDARY_NOTE,
                },
            },
        }],
    }))
}

pub fn render_json(payload: &Value) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(payload)?))
}
