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
