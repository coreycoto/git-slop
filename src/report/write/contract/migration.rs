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
        if repo
            .get("head_sha")
            .and_then(Value::as_str)
            .is_some_and(|value| value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            repo.insert("head_sha".to_string(), Value::Null);
        }
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
                    ("generated_from", json!([])),
                    ("generated_provenance", json!({"source_paths": [], "source_globs": [], "generator_command": null, "verification_command": null})),
                    ("has_inline_tests", json!(false)),
                    ("tokens", json!(tokens)),
                    ("context_pressure", json!(0.0)),
                    ("content_fingerprint", json!("")),
                    (
                        "content_sha256",
                        json!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
                    ),
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
                if file
                    .get("content_fingerprint")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
                {
                    file.insert("content_fingerprint".to_string(), json!(zero_digest.clone()));
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
                    "classification",
                    source
                        .get("classification")
                        .cloned()
                        .unwrap_or_else(|| json!("other")),
                ),
                (
                    "generated_from",
                    source
                        .get("generated_from")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                ),
                ("synchronization_group", Value::Null),
                ("remediation_kind", json!("investigation")),
                ("remediation_target_paths", json!([path])),
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
    root.entry("observation_feed")
        .or_insert_with(|| json!([]));
    let ranked_files = root
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|file| {
            json!({
                "path": file.get("path").cloned().unwrap_or(Value::Null),
                "classification": file.get("classification").cloned().unwrap_or_else(|| json!("other")),
                "profile": file.get("profile").cloned().unwrap_or_else(|| json!("agent_context")),
                "remediation_kind": "investigation",
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
            "content_sha256": record.get("content_sha256").cloned().unwrap_or(Value::Null),
            "analysis_status": record.get("analysis_status").cloned().unwrap_or_else(|| json!("legacy_unknown")),
            "skipped_reason": record.get("skipped_reason").cloned().unwrap_or(Value::Null),
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
    if let Some(evidence) = root
        .get_mut("evidence_completeness")
        .and_then(Value::as_object_mut)
    {
        for (key, value) in [
            ("history", json!("legacy_unknown")),
            ("repository_size", json!("legacy_unknown")),
            ("history_window_days", Value::Null),
            ("history_max_commits", Value::Null),
            ("first_seen_age", json!("legacy_unknown")),
            ("churn_window", json!("legacy_unknown")),
            ("author_evidence", json!("legacy_unknown")),
            ("relationship_evidence", json!("legacy_unknown")),
            ("missing_test_evidence_count", json!(0)),
            ("weak_test_mapping_count", json!(0)),
            ("low_test_cochange_evidence_count", json!(0)),
            ("relationship_support", json!("legacy_unknown")),
        ] {
            evidence.entry(key).or_insert(value);
        }
    }
    let tracked_file_count = root
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let stats = root
        .entry("stats")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("stats must be an object"))?;
    for (key, value) in [
        ("tracked_file_count", json!(tracked_file_count)),
        ("analyzed_file_count", json!(tracked_file_count)),
        ("skipped_ignored_count", json!(0)),
        ("skipped_missing_count", json!(0)),
        ("skipped_binary_count", json!(0)),
        ("skipped_undecodable_count", json!(0)),
        ("critical_context_file_count", json!(0)),
        ("critical_slop_file_count", json!(0)),
        ("history_complete", json!(false)),
        ("migration_status", json!("legacy_unknown")),
    ] {
        stats.entry(key).or_insert(value);
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
    root.entry("summary").or_insert_with(|| json!({}));
    if let Some(summary) = root.get_mut("summary").and_then(Value::as_object_mut) {
        summary.retain(|key, _| {
            matches!(
                key.as_str(),
                "top_hotspots" | "top_structural_files" | "top_verification_gaps" | "health"
            )
        });
        for (key, value) in [
            ("top_hotspots", json!([])),
            ("top_structural_files", json!([])),
            ("top_verification_gaps", json!([])),
            (
                "health",
                json!({"file_band_counts": {}, "folder_band_counts": {}}),
            ),
        ] {
            summary.entry(key).or_insert(value);
        }
    }
    let mut migrated = Value::Object(root.clone());
    normalize_legacy_scalar_contracts(&mut migrated);
    Ok(migrated)
}

fn normalize_legacy_scalar_contracts(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_legacy_scalar_contracts(value);
            }
        }
        Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                match key.as_str() {
                    "context_band" => {
                        let canonical = match value.as_str() {
                            Some("compact" | "healthy" | "warning" | "critical") => None,
                            Some("refactor_required") => Some("critical"),
                            _ => Some("compact"),
                        };
                        if let Some(canonical) = canonical {
                            *value = json!(canonical);
                        }
                    }
                    "health_band" => {
                        let canonical = match value.as_str() {
                            Some("compact" | "healthy" | "warning" | "budget_exceeded") => None,
                            Some("refactor_required" | "critical") => Some("budget_exceeded"),
                            _ => Some("compact"),
                        };
                        if let Some(canonical) = canonical {
                            *value = json!(canonical);
                        }
                    }
                    "slop_band" => {
                        let canonical = match value.as_str() {
                            Some("low" | "moderate" | "high") => None,
                            Some("critical" | "warning") => Some("high"),
                            _ => Some("low"),
                        };
                        if let Some(canonical) = canonical {
                            *value = json!(canonical);
                        }
                    }
                    "slop_score" => {
                        let score = value.as_f64().unwrap_or_default().clamp(0.0, 100.0);
                        *value = json!(score);
                    }
                    _ => {}
                }
                normalize_legacy_scalar_contracts(value);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn health_band_migration_preserves_budget_failures_separately_from_context() {
        let mut value = json!({
            "context_band": "budget_exceeded",
            "health_band": "budget_exceeded",
            "legacy": {"health_band": "critical"}
        });
        normalize_legacy_scalar_contracts(&mut value);
        assert_eq!(value["context_band"], "compact");
        assert_eq!(value["health_band"], "budget_exceeded");
        assert_eq!(value["legacy"]["health_band"], "budget_exceeded");
    }
}
