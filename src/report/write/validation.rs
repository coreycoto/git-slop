#[derive(Debug, Serialize)]
pub(crate) struct ValidationIssue {
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

fn collect_classification_issue(
    issues: &mut Vec<ValidationIssue>,
    record: &Value,
    pointer: String,
    allow_mixed: bool,
) {
    let classification = record.get("classification").and_then(Value::as_str);
    if !classification.is_some_and(|value| {
        crate::model::Classification::is_valid(value) || (allow_mixed && value == "mixed")
    }) {
        issues.push(ValidationIssue {
            code: "invalid_classification",
            pointer,
            message: "classification must use the canonical classification enum".to_string(),
        });
    }
}

pub(crate) fn validation_issues(report: &Value) -> Vec<ValidationIssue> {
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
        "observation_feed",
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
        "policy_index",
        "ranked_files",
        "action_queue",
        "observation_feed",
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
            "compact_profile_note",
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
            "incomplete_inventory_files",
            "intentionally_skipped_non_text_files",
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
        "evidence_lower_bound",
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
    if let Some(policy_index) = root.get("policy_index") {
        let Some(policy_index) = policy_index.as_object() else {
            issues.push(ValidationIssue {
                code: "type_mismatch",
                pointer: "/policy_index".to_string(),
                message: "policy_index must be an object".to_string(),
            });
            return issues;
        };
        for collection in ["files", "folders"] {
            match policy_index.get(collection).and_then(Value::as_array) {
                Some(records) => {
                    for (index, record) in records.iter().enumerate() {
                        for field in [
                            "path",
                            "classification",
                            "tokens",
                            "context_band",
                            "slop_score",
                            "slop_band",
                            "reason_codes",
                        ] {
                            if record.get(field).is_none() {
                                issues.push(ValidationIssue {
                                    code: "required_field_missing",
                                    pointer: format!("/policy_index/{collection}/{index}/{field}"),
                                    message: format!("{field} is required in policy records"),
                                });
                            }
                        }
                        collect_classification_issue(
                            &mut issues,
                            record,
                            format!("/policy_index/{collection}/{index}/classification"),
                            collection == "folders",
                        );
                    }
                }
                None => issues.push(ValidationIssue {
                    code: "type_mismatch",
                    pointer: format!("/policy_index/{collection}"),
                    message: format!("policy_index.{collection} must be an array"),
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
        "generated_from",
        "analysis_status",
        "skipped_reason",
        "symlink_metadata",
        "has_inline_tests",
        "tokens",
        "context_band",
        "context_pressure",
        "content_fingerprint",
        "content_sha256",
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
            collect_classification_issue(
                &mut issues,
                record,
                format!("/{collection}/{index}/classification"),
                collection == "folders",
            );
        }
    }
    let queue_fields = [
        "path",
        "profile",
        "classification",
        "generated_from",
        "synchronization_group",
        "remediation_kind",
        "remediation_target_paths",
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
        "classification",
        "profile",
        "remediation_kind",
        "slop_score",
        "slop_band",
        "context_band",
        "tokens",
        "reason_codes",
    ];
    for (collection, fields) in [
        ("action_queue", queue_fields.as_slice()),
        ("observation_feed", queue_fields.as_slice()),
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
            collect_classification_issue(
                &mut issues,
                record,
                format!("/{collection}/{index}/classification"),
                false,
            );
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

pub fn validation_violations(report: &Value) -> Vec<Value> {
    validation_issues(report)
        .into_iter()
        .filter_map(|issue| serde_json::to_value(issue).ok())
        .collect()
}
