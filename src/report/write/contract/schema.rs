pub fn schema() -> Value {
    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/coreycoto/git-slop/blob/v0.11.6/schemas/report-5.json",
        "title": "Git Slop report schema 5",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "analyzer", "generated_at", "analyzed_revision_at", "repo", "scope", "config", "stats", "summary", "files", "folders", "ranked_files", "action_queue", "observation_feed", "costs", "overlays", "health", "diagnostics", "collection_metadata", "evidence_completeness", "terminology"],
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
            "policy_evaluation": {"type": "object", "additionalProperties": false, "required": ["policy_failures", "intervention_candidates", "advisory_findings", "emitted_annotations", "thresholds", "count_semantics"], "properties": {"policy_failures":{"type":"integer","minimum":0},"intervention_candidates":{"type":"integer","minimum":0},"advisory_findings":{"type":"integer","minimum":0},"emitted_annotations":{"type":"object"},"thresholds":{"type":"object"},"count_semantics":{"type":"string"}}},
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
            "policy_index": {
                "type": "object",
                "additionalProperties": false,
                "required": ["files", "folders"],
                "properties": {
                    "files": {"type": "array", "items": {"$ref": "#/$defs/policy_record"}},
                    "folders": {"type": "array", "items": {"$ref": "#/$defs/policy_record"}}
                }
            },
            "action_queue": {"type": "array", "items": {"$ref": "#/$defs/queue_item"}},
            "observation_feed": {"type": "array", "items": {"$ref": "#/$defs/queue_item"}},
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
            "classification": {"type":"string","enum": crate::model::Classification::values()},
            "costs": {"type":"object","additionalProperties":false,"properties":{"load":{"type":"object","additionalProperties":false,"properties":{"analysis_status":{"type":"string"},"analysis_version":{"type":"integer"},"file_token_count":{"type":"integer"},"folder_token_count":{"type":"integer"},"top_file_share":{"type":"number"},"top_3_file_share":{"type":"number"},"token_concentration_ratio":{"type":"number"},"context_band":{"type":"string"},"load_pressure":{"type":"number"}}},"volatility":{"type":"object","additionalProperties":false,"properties":{"analysis_status":{"type":"string"},"analysis_version":{"type":"integer"},"commit_count_window":{"type":"number"},"recency_weighted_commits":{"type":"number"},"line_churn_window":{"type":"number"},"token_churn_window":{"type":"number"},"relative_token_churn":{"type":"number"},"late_churn_spike":{"type":"number"},"volatility_pressure":{"type":"number"},"churn_measurement":{"type":"string"}}},"coordination":{"type":"object","additionalProperties":false,"properties":{"analysis_status":{"type":"string"},"analysis_version":{"type":"integer"},"files_touched_per_change":{"type":"number"},"folders_touched_per_change":{"type":"number"},"edit_hunks_per_change":{"type":"number"},"change_diffusion":{"type":"number"},"cochange_degree":{"type":"number"},"cochange_centrality":{"type":"number"},"cochange_pagerank":{"type":"number"},"cross_folder_cochange_ratio":{"type":"number"},"coordination_pressure":{"type":"number"}}}}},
            "compare_record": {"type":"object","additionalProperties":false,"required":["path","content_fingerprint","content_sha256","analysis_status","skipped_reason","tokens","context_band","slop_score","slop_band","costs","overlays"],"properties":{"path":{"type":"string"},"content_fingerprint":{"type":["string","null"]},"content_sha256":{"type":["string","null"],"pattern":"^[0-9a-f]{64}$"},"analysis_status":{"type":"string"},"skipped_reason":{"type":["string","null"]},"tokens":{"type":"integer","minimum":0},"context_band":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "generated_provenance": {"type":"object","additionalProperties":false,"required":["source_paths","source_globs","generator_command","verification_command"],"properties":{"source_paths":{"type":"array","items":{"type":"string"}},"source_globs":{"type":"array","items":{"type":"string"}},"generator_command":{"type":["string","null"]},"verification_command":{"type":["string","null"]}}},
            "policy_record": {"type":"object","additionalProperties":false,"required":["path","classification","profile","generated_from","tokens","context_band","slop_score","slop_band","reason_codes"],"properties":{"path":{"type":"string"},"classification":{"$ref":"#/$defs/classification"},"profile":{"type":["string","null"]},"generated_from":{"type":"array","items":{"type":"string"}},"generated_provenance":{"$ref":"#/$defs/generated_provenance"},"tokens":{"type":"integer","minimum":0},"context_band":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array","items":{"type":"string"}}}},
            "file": {"type": "object", "additionalProperties": false, "required": ["path", "bytes", "lines", "blank_lines", "code_lines", "comment_lines", "language", "profile", "classification", "generated_from", "analysis_status", "skipped_reason", "symlink_metadata", "has_inline_tests", "tokens", "context_band", "context_pressure", "content_fingerprint", "content_sha256", "structural_token_count", "top_structural_terms", "structural_categories", "age_days", "revisions_window", "recency_weighted_commits", "added_window", "deleted_window", "churn_lines_window", "line_churn_window", "token_churn_window", "relative_churn_window", "late_churn_spike", "author_count_window", "author_entropy", "top_author_share", "days_since_non_bot_edit", "recent_maintainer_diversity", "age_pressure", "revision_norm", "relative_churn_norm", "churn_pressure", "slop_score", "slop_band", "reason_codes", "costs", "overlays"], "properties": {"path":{"type":"string"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"blank_lines":{"type":"integer"},"code_lines":{"type":"integer"},"comment_lines":{"type":"integer"},"language":{"type":"string"},"profile":{"type":"string"},"classification":{"type":"string"},"generated_from":{"type":"array","items":{"type":"string"}},"generated_provenance":{"$ref":"#/$defs/generated_provenance"},"analysis_status":{"type":"string"},"skipped_reason":{"type":["string","null"]},"symlink_metadata":{"type":["object","null"]},"has_inline_tests":{"type":"boolean"},"tokens":{"type":"integer"},"context_band":{"type":"string"},"context_pressure":{"type":"number"},"content_fingerprint":{"type":"string"},"content_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},"structural_token_count":{"type":"integer"},"top_structural_terms":{"type":"array"},"structural_categories":{"type":"object"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"recency_weighted_commits":{"type":"number"},"added_window":{"type":"integer"},"deleted_window":{"type":"integer"},"churn_lines_window":{"type":"integer"},"line_churn_window":{"type":"integer"},"token_churn_window":{"type":"integer"},"relative_churn_window":{"type":"number"},"late_churn_spike":{"type":"number"},"author_count_window":{"type":"integer"},"author_entropy":{"type":"number"},"top_author_share":{"type":"number"},"days_since_non_bot_edit":{"type":["integer","null"]},"recent_maintainer_diversity":{"type":"integer"},"age_pressure":{"type":"number"},"revision_norm":{"type":"number"},"relative_churn_norm":{"type":"number"},"churn_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "folder": {"type": "object", "additionalProperties": false, "required": ["path", "descendant_file_count", "direct_file_count", "bytes", "lines", "tokens", "direct_tokens", "context_band", "health_band", "context_pressure", "slop_score", "slop_band", "reason_codes", "top_file_path", "classification", "costs", "overlays"], "properties":{"path":{"type":"string"},"descendant_file_count":{"type":"integer"},"direct_file_count":{"type":"integer"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"tokens":{"type":"integer"},"direct_tokens":{"type":"integer"},"context_band":{"type":"string"},"health_band":{"type":"string"},"context_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"top_file_path":{"type":"string"},"classification":{"type":"string"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "queue_item": {"type": "object", "additionalProperties": false, "required": ["path", "profile", "classification", "generated_from", "synchronization_group", "remediation_kind", "slop_score", "slop_band", "context_band", "tokens", "age_days", "revisions_window", "churn_pressure", "reason_codes", "is_pure_context_hotspot", "severity", "evidence_status", "next_action"], "properties":{"path":{"type":"string"},"profile":{"enum":["agent_context","data_context"]},"classification":{"type":"string"},"generated_from":{"type":"array","items":{"type":"string"}},"synchronization_group":{"type":["string","null"]},"remediation_kind":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"tokens":{"type":"integer"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"churn_pressure":{"type":"number"},"reason_codes":{"type":"array","items":{"type":"string"}},"is_pure_context_hotspot":{"type":"boolean"},"severity":{"enum":["error","warning","notice"]},"evidence_status":{"type":"string"},"next_action":{"type":"string"}}},
            "ranked_file": {"type": "object", "additionalProperties": false, "required": ["path", "slop_score", "slop_band", "context_band", "tokens", "reason_codes"], "properties":{"path":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"tokens":{"type":"integer"},"reason_codes":{"type":"array","items":{"type":"string"}}}}
        }
    });
    apply_shared_classification_schema(&mut schema);
    harden_generated_contracts(&mut schema);
    harden_scalar_contracts(&mut schema);
    schema
}

fn harden_scalar_contracts(schema: &mut Value) {
    schema["$defs"]["profile"] = json!({"enum":["agent_context","data_context"]});
    schema["$defs"]["context_band"] =
        json!({"enum":["compact","healthy","warning","critical"]});
    schema["$defs"]["slop_band"] = json!({"enum":["low","moderate","high"]});
    schema["$defs"]["analysis_status"] = json!({"enum":[
        "analyzed","skipped","stable","experimental","not_applicable","legacy_unknown",
        "complete","degraded_resource_budget","degraded_large_files","degraded_incomplete_inventory"
    ]});
    schema["$defs"]["evidence_status"] = json!({"enum":[
        "supported","limited","low_support","not_applicable","evidence_unavailable",
        "mapping_confidence_low","evidence_found","no_mapping","no_evidence","legacy_unknown"
    ]});
    schema["$defs"]["sha1"] = json!({"type":"string","pattern":"^[0-9a-f]{40}$"});
    schema["$defs"]["digest"] = json!({"type":"string","pattern":"^[0-9a-f]{64}$"});
    schema["$defs"]["fingerprint"] = json!({
        "oneOf": [
            {"$ref":"#/$defs/digest"},
            {"type":"string","pattern":"^incomplete:[a-z_]+:[0-9]+$"}
        ]
    });

    fn visit(value: &mut Value) {
        let Some(object) = value.as_object_mut() else {
            if let Some(values) = value.as_array_mut() {
                for child in values {
                    visit(child);
                }
            }
            return;
        };
        if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
            for (name, property) in properties.iter_mut() {
                let nullable = property
                    .get("type")
                    .and_then(Value::as_array)
                    .is_some_and(|types| types.iter().any(|kind| kind == "null"));
                let typed = |reference: &str| {
                    if nullable {
                        json!({"oneOf":[{"$ref":reference},{"type":"null"}]})
                    } else {
                        json!({"$ref":reference})
                    }
                };
                let replacement = match name.as_str() {
                    "profile" => Some(typed("#/$defs/profile")),
                    "context_band" | "health_band" => {
                        Some(typed("#/$defs/context_band"))
                    }
                    "slop_band" => Some(typed("#/$defs/slop_band")),
                    "analysis_status" => Some(typed("#/$defs/analysis_status")),
                    "evidence_status" => Some(typed("#/$defs/evidence_status")),
                    "head_sha" => Some(typed("#/$defs/sha1")),
                    "content_sha256" => Some(typed("#/$defs/digest")),
                    "content_fingerprint" => Some(typed("#/$defs/fingerprint")),
                    "worktree_state_digest" | "analyzed_content_digest"
                    | "selected_path_digest" | "config_digest" | "analysis_config_digest"
                    | "evidence_config_digest" | "policy_config_digest"
                    | "presentation_config_digest" => Some(typed("#/$defs/digest")),
                    "slop_score" => Some(json!({"type":"number","minimum":0,"maximum":100})),
                    "context_pressure" | "churn_pressure" | "load_pressure"
                    | "volatility_pressure" | "coordination_pressure" | "top_file_share"
                    | "top_3_file_share" | "token_concentration_ratio" | "top_author_share"
                    | "late_churn_spike" | "cochange_centrality" | "cochange_pagerank"
                    | "cross_folder_cochange_ratio" | "change_diffusion" | "evidence_score"
                    | "similarity" | "similarity_ratio" | "jaccard" | "calibrated_jaccard"
                    | "evidence_lower_bound" | "confidence_lower_bound" => {
                        Some(json!({"type":"number","minimum":0,"maximum":1}))
                    }
                    _ => None,
                };
                if let Some(replacement) = replacement {
                    *property = replacement;
                }
                visit(property);
            }
        }
        for (key, child) in object.iter_mut() {
            if key != "properties" {
                visit(child);
            }
        }
    }
    visit(schema);
}

fn apply_shared_classification_schema(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(classification) = object
                .get_mut("properties")
                .and_then(Value::as_object_mut)
                .and_then(|properties| properties.get_mut("classification"))
            {
                *classification = json!({"$ref": "#/$defs/classification"});
            }
            for child in object.values_mut() {
                apply_shared_classification_schema(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                apply_shared_classification_schema(child);
            }
        }
        _ => {}
    }
}

fn harden_generated_contracts(schema: &mut Value) {
    let mut aggregate_classifications = crate::model::Classification::values();
    aggregate_classifications.push("mixed");
    schema["$defs"]["classification_or_mixed"] =
        json!({"type":"string","enum":aggregate_classifications});
    schema["$defs"]["json_value"] = json!({
        "oneOf": [
            {"type":"null"}, {"type":"boolean"}, {"type":"number"}, {"type":"string"},
            {"type":"array","items":{"$ref":"#/$defs/json_value"}},
            {"$ref":"#/$defs/json_object"}
        ]
    });
    schema["$defs"]["json_object"] = json!({
        "type":"object","additionalProperties":{"$ref":"#/$defs/json_value"}
    });
    let mut config_schema = crate::config::schema();
    if let Some(object) = config_schema.as_object_mut() {
        for key in ["$schema", "$id", "title", "description", "x-git-slop-deprecated-keys"] {
            object.remove(key);
        }
    }
    schema["properties"]["config"] = config_schema;
    schema["properties"]["stats"] = json!({
        "type":"object","additionalProperties":false,
        "required":["tracked_file_count","analyzed_file_count","skipped_ignored_count","skipped_missing_count","skipped_binary_count","skipped_undecodable_count","critical_context_file_count","critical_slop_file_count","history_complete"],
        "properties":{
            "tracked_file_count":{"type":"integer","minimum":0},"analyzed_file_count":{"type":"integer","minimum":0},
            "skipped_ignored_count":{"type":"integer","minimum":0},"skipped_missing_count":{"type":"integer","minimum":0},
            "skipped_binary_count":{"type":"integer","minimum":0},"skipped_undecodable_count":{"type":"integer","minimum":0},
            "critical_context_file_count":{"type":"integer","minimum":0},"critical_slop_file_count":{"type":"integer","minimum":0},
            "history_complete":{"type":"boolean"},"migration_status":{"type":"string"}
        }
    });
    schema["properties"]["summary"] = json!({
        "type":"object","additionalProperties":false,
        "required":["top_hotspots","top_structural_files","top_verification_gaps","health"],
        "properties":{
            "top_hotspots":{"type":"array","items":{"type":"string"}},
            "top_structural_files":{"type":"array","items":{"type":"string"}},
            "top_verification_gaps":{"type":"array","items":{"type":"string"}},
            "health":{"type":"object","additionalProperties":false,"required":["file_band_counts","folder_band_counts"],"properties":{
                "file_band_counts":{"$ref":"#/$defs/band_counts"},"folder_band_counts":{"$ref":"#/$defs/band_counts"}
            }}
        }
    });
    schema["properties"]["overlays"] = json!({"$ref":"#/$defs/json_object"});
    schema["properties"]["diagnostics"] = json!({
        "type":"object","additionalProperties":false,
        "properties":{
            "suppressed_saturated_overlays":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["overlay","reason","measured_count","range","entropy_bits"],"properties":{
                "overlay":{"type":"string"},"reason":{"enum":["saturated","low_variance","low_entropy"]},
                "measured_count":{"type":"integer","minimum":0},"range":{"type":"number"},"entropy_bits":{"type":"number","minimum":0}
            }}},
            "relationship_count":{"type":"integer","minimum":0},
            "structural_token_payload_omitted":{"type":"boolean"},
            "analysis":{"$ref":"#/$defs/json_object"},
            "compact_profile_note":{"type":"string"},"evidence_limit":{"type":"string"},
            "migration":{"type":"string"},"report_profile":{"enum":["compact","standard","full_evidence"]},
            "report_profile_semantics":{"type":"string"},
            "report_sizes":{"type":"object","additionalProperties":false,"required":["report_json_bytes","report_yaml_bytes","logical_artifact_bytes","physical_storage_semantics"],"properties":{
                "report_json_bytes":{"type":"integer","minimum":0},"report_yaml_bytes":{"type":"integer","minimum":0},
                "logical_artifact_bytes":{"type":"integer","minimum":0},"physical_storage_semantics":{"type":"string"}
            }}
        }
    });
    schema["properties"]["evidence_completeness"] = json!({
        "type":"object","additionalProperties":false,
        "required":["history","repository_size","history_window_days","history_max_commits","first_seen_age","churn_window","author_evidence","relationship_evidence","missing_test_evidence_count","weak_test_mapping_count","low_test_cochange_evidence_count","relationship_support"],
        "properties":{
            "history":{"type":"string"},"repository_size":{"type":"string"},
            "history_window_days":{"type":["integer","null"],"minimum":0},"history_max_commits":{"type":["integer","null"],"minimum":0},
            "first_seen_age":{"type":"string"},"churn_window":{"type":"string"},"author_evidence":{"type":"string"},
            "relationship_evidence":{"type":"string"},"missing_test_evidence_count":{"type":"integer","minimum":0},
            "weak_test_mapping_count":{"type":"integer","minimum":0},"low_test_cochange_evidence_count":{"type":"integer","minimum":0},
            "relationship_support":{"type":"string"}
        }
    });
    schema["properties"]["terminology"] = json!({
        "type":"object","additionalProperties":false,
        "required":["attention_required","budget_exceeded","critical","error"],
        "properties":{
            "attention_required":{"type":"string"},"budget_exceeded":{"type":"string"},
            "critical":{"type":"string"},"error":{"type":"string"}
        }
    });
    schema["$defs"]["band_counts"] = json!({
        "type":"object","additionalProperties":{"type":"integer","minimum":0}
    });
    schema["$defs"]["collection_page"] = json!({
        "type":"object","additionalProperties":false,"required":["total","returned","limit","truncated"],
        "properties":{"total":{"type":"integer","minimum":0},"returned":{"type":"integer","minimum":0},"limit":{"type":["integer","null"],"minimum":0},"truncated":{"type":"boolean"},"low_support_aggregated":{"type":"integer","minimum":0},"scope":{"type":"string"}}
    });
    schema["$defs"]["index_collection_metadata"] = json!({
        "type":"object","additionalProperties":false,"required":["files","folders"],
        "properties":{"files":{"$ref":"#/$defs/collection_page"},"folders":{"$ref":"#/$defs/collection_page"}}
    });
    schema["properties"]["collection_metadata"] = json!({
        "type":"object","additionalProperties":{"$ref":"#/$defs/collection_page"},
        "properties":{
            "files":{"$ref":"#/$defs/collection_page"},"folders":{"$ref":"#/$defs/collection_page"},
            "compare_index":{"$ref":"#/$defs/index_collection_metadata"},"policy_index":{"$ref":"#/$defs/index_collection_metadata"},
            "action_queue":{"$ref":"#/$defs/collection_page"},"observation_feed":{"$ref":"#/$defs/collection_page"},
            "ranked_files":{"$ref":"#/$defs/collection_page"},"health.findings":{"$ref":"#/$defs/collection_page"},
            "health.refactor_candidates":{"$ref":"#/$defs/collection_page"},"health.watchlist":{"$ref":"#/$defs/collection_page"}
        }
    });
    if let Some(properties) = schema
        .pointer_mut("/$defs/queue_item/properties")
        .and_then(Value::as_object_mut)
    {
        properties.insert(
            "remediation_target_paths".to_string(),
            json!({"type":"array","items":{"type":"string"}}),
        );
    }
    if let Some(required) = schema
        .pointer_mut("/$defs/queue_item/required")
        .and_then(Value::as_array_mut)
    {
        required.push(json!("remediation_target_paths"));
    }
    if let Some(properties) = schema
        .pointer_mut("/$defs/ranked_file/properties")
        .and_then(Value::as_object_mut)
    {
        properties.insert(
            "classification".to_string(),
            json!({"$ref":"#/$defs/classification"}),
        );
        properties.insert("profile".to_string(), json!({"type":"string"}));
        properties.insert("remediation_kind".to_string(), json!({"type":"string"}));
    }
    if let Some(required) = schema
        .pointer_mut("/$defs/ranked_file/required")
        .and_then(Value::as_array_mut)
    {
        required.extend([
            json!("classification"),
            json!("profile"),
            json!("remediation_kind"),
        ]);
    }
    schema["$defs"]["finding"] = json!({
        "type":"object",
        "additionalProperties":false,
        "required":["path","profile","severity","title","message","next_command","slop_band","context_band","slop_score","tokens","reasons"],
        "properties":{
            "path":{"type":"string"},"profile":{"type":"string"},"severity":{"enum":["error","warning","notice"]},"title":{"type":"string"},"message":{"type":"string"},"next_command":{"type":"string"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"slop_score":{"type":"number"},"tokens":{"type":"integer","minimum":0},"reasons":{"type":"array","items":{"type":"string"}}
        }
    });
    schema["$defs"]["totals"] = json!({
        "type":"object","additionalProperties":false,"required":["files","lines","code","comments","blanks","tokens"],
        "properties":{"files":{"type":"integer","minimum":0},"lines":{"type":"integer","minimum":0},"code":{"type":"integer","minimum":0},"comments":{"type":"integer","minimum":0},"blanks":{"type":"integer","minimum":0},"tokens":{"type":"integer","minimum":0}}
    });
    schema["$defs"]["profile_rollup"] = json!({
        "type":"object","additionalProperties":false,"required":["name","totals"],
        "properties":{"name":{"type":"string"},"totals":{"$ref":"#/$defs/totals"}}
    });
    schema["$defs"]["language_rollup"] = json!({
        "type":"object","additionalProperties":false,"required":["profile","language","files","lines","code","comments","blanks","tokens","token_share"],
        "properties":{"profile":{"type":"string"},"language":{"type":"string"},"files":{"type":"integer","minimum":0},"lines":{"type":"integer","minimum":0},"code":{"type":"integer","minimum":0},"comments":{"type":"integer","minimum":0},"blanks":{"type":"integer","minimum":0},"tokens":{"type":"integer","minimum":0},"token_share":{"type":"number","minimum":0,"maximum":1}}
    });
    schema["$defs"]["distribution"] = json!({
        "type":"object","additionalProperties":false,"required":["count","total","p50","p90","p95","p99","max","top_1_share","top_5_share","top_10_share"],
        "properties":{"count":{"type":"integer","minimum":0},"total":{"type":"integer","minimum":0},"p50":{"type":"number","minimum":0},"p90":{"type":"number","minimum":0},"p95":{"type":"number","minimum":0},"p99":{"type":"number","minimum":0},"max":{"type":"integer","minimum":0},"top_1_share":{"type":"number","minimum":0,"maximum":1},"top_5_share":{"type":"number","minimum":0,"maximum":1},"top_10_share":{"type":"number","minimum":0,"maximum":1}}
    });
    schema["$defs"]["candidate"] = json!({
        "type":"object","additionalProperties":false,
        "required":["kind","path","profile","class","tokens","parent_tokens","band","slop_band","slop_score","reason_codes"],
        "properties":{"kind":{"enum":["file","folder"]},"path":{"type":"string"},"profile":{"type":"string"},"class":{"$ref":"#/$defs/classification_or_mixed"},"files":{"type":"integer","minimum":0},"descendant_files":{"type":"integer","minimum":0},"tokens":{"type":"integer","minimum":0},"recursive_tokens":{"type":"integer","minimum":0},"parent_tokens":{"type":"integer","minimum":0},"band":{"type":"string"},"slop_band":{"type":"string"},"slop_score":{"type":"number"},"reason_codes":{"type":"array","items":{"type":"string"}}}
    });
    schema["$defs"]["health"] = json!({
        "type":"object",
        "additionalProperties":false,
        "required":["file_band_counts","folder_band_counts","profile_rollups","language_rollups","file_distribution","folder_distribution","refactor_candidates","watchlist","findings"],
        "properties":{
            "file_band_counts":{"$ref":"#/$defs/band_counts"},
            "folder_band_counts":{"$ref":"#/$defs/band_counts"},
            "profile_rollups":{"type":"array","items":{"$ref":"#/$defs/profile_rollup"}},
            "language_rollups":{"type":"object","additionalProperties":{"type":"array","items":{"$ref":"#/$defs/language_rollup"}}},
            "file_distribution":{"$ref":"#/$defs/distribution"},"folder_distribution":{"$ref":"#/$defs/distribution"},
            "refactor_candidates":{"type":"array","items":{"$ref":"#/$defs/candidate"}},
            "watchlist":{"type":"array","items":{"$ref":"#/$defs/candidate"}},
            "findings":{"type":"array","items":{"$ref":"#/$defs/finding"}}
        }
    });
    schema["properties"]["health"] = json!({"$ref":"#/$defs/health"});
    for pointer in [
        "/$defs/file/properties/overlays", "/$defs/folder/properties/overlays",
        "/$defs/compare_record/properties/overlays"
    ] {
        if let Some(value) = schema.pointer_mut(pointer) {
            *value = json!({"$ref":"#/$defs/json_object"});
        }
    }
    schema["$defs"]["file"]["properties"]["top_structural_terms"] =
        json!({"type":"array","items":{"type":"string"}});
    schema["$defs"]["file"]["properties"]["reason_codes"] =
        json!({"type":"array","items":{"type":"string"}});
    schema["$defs"]["folder"]["properties"]["reason_codes"] =
        json!({"type":"array","items":{"type":"string"}});
    schema["$defs"]["folder"]["properties"]["classification"] =
        json!({"$ref":"#/$defs/classification_or_mixed"});
    schema["$defs"]["policy_record"]["properties"]["classification"] =
        json!({"$ref":"#/$defs/classification_or_mixed"});
    schema["$defs"]["file"]["properties"]["structural_categories"] =
        json!({"$ref":"#/$defs/json_object"});
    schema["$defs"]["file"]["properties"]["symlink_metadata"] =
        json!({"type":["object","null"],"additionalProperties":{"$ref":"#/$defs/json_value"}});
}
