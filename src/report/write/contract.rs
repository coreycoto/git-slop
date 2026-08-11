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
            "policy_record": {"type":"object","additionalProperties":false,"required":["path","classification","profile","generated_from","tokens","context_band","slop_score","slop_band","reason_codes"],"properties":{"path":{"type":"string"},"classification":{"$ref":"#/$defs/classification"},"profile":{"type":["string","null"]},"generated_from":{"type":"array","items":{"type":"string"}},"tokens":{"type":"integer","minimum":0},"context_band":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array","items":{"type":"string"}}}},
            "file": {"type": "object", "additionalProperties": false, "required": ["path", "bytes", "lines", "blank_lines", "code_lines", "comment_lines", "language", "profile", "classification", "generated_from", "analysis_status", "skipped_reason", "symlink_metadata", "has_inline_tests", "tokens", "context_band", "context_pressure", "content_fingerprint", "content_sha256", "structural_token_count", "top_structural_terms", "structural_categories", "age_days", "revisions_window", "recency_weighted_commits", "added_window", "deleted_window", "churn_lines_window", "line_churn_window", "token_churn_window", "relative_churn_window", "late_churn_spike", "author_count_window", "author_entropy", "top_author_share", "days_since_non_bot_edit", "recent_maintainer_diversity", "age_pressure", "revision_norm", "relative_churn_norm", "churn_pressure", "slop_score", "slop_band", "reason_codes", "costs", "overlays"], "properties": {"path":{"type":"string"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"blank_lines":{"type":"integer"},"code_lines":{"type":"integer"},"comment_lines":{"type":"integer"},"language":{"type":"string"},"profile":{"type":"string"},"classification":{"type":"string"},"generated_from":{"type":"array","items":{"type":"string"}},"analysis_status":{"type":"string"},"skipped_reason":{"type":["string","null"]},"symlink_metadata":{"type":["object","null"]},"has_inline_tests":{"type":"boolean"},"tokens":{"type":"integer"},"context_band":{"type":"string"},"context_pressure":{"type":"number"},"content_fingerprint":{"type":"string"},"content_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},"structural_token_count":{"type":"integer"},"top_structural_terms":{"type":"array"},"structural_categories":{"type":"object"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"recency_weighted_commits":{"type":"number"},"added_window":{"type":"integer"},"deleted_window":{"type":"integer"},"churn_lines_window":{"type":"integer"},"line_churn_window":{"type":"integer"},"token_churn_window":{"type":"integer"},"relative_churn_window":{"type":"number"},"late_churn_spike":{"type":"number"},"author_count_window":{"type":"integer"},"author_entropy":{"type":"number"},"top_author_share":{"type":"number"},"days_since_non_bot_edit":{"type":["integer","null"]},"recent_maintainer_diversity":{"type":"integer"},"age_pressure":{"type":"number"},"revision_norm":{"type":"number"},"relative_churn_norm":{"type":"number"},"churn_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "folder": {"type": "object", "additionalProperties": false, "required": ["path", "descendant_file_count", "direct_file_count", "bytes", "lines", "tokens", "direct_tokens", "context_band", "health_band", "context_pressure", "slop_score", "slop_band", "reason_codes", "top_file_path", "classification", "costs", "overlays"], "properties":{"path":{"type":"string"},"descendant_file_count":{"type":"integer"},"direct_file_count":{"type":"integer"},"bytes":{"type":"integer"},"lines":{"type":"integer"},"tokens":{"type":"integer"},"direct_tokens":{"type":"integer"},"context_band":{"type":"string"},"health_band":{"type":"string"},"context_pressure":{"type":"number"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"reason_codes":{"type":"array"},"top_file_path":{"type":"string"},"classification":{"type":"string"},"costs":{"$ref":"#/$defs/costs"},"overlays":{"type":"object"}}},
            "queue_item": {"type": "object", "additionalProperties": false, "required": ["path", "profile", "classification", "generated_from", "synchronization_group", "remediation_kind", "slop_score", "slop_band", "context_band", "tokens", "age_days", "revisions_window", "churn_pressure", "reason_codes", "is_pure_context_hotspot", "severity", "evidence_status", "next_action"], "properties":{"path":{"type":"string"},"profile":{"enum":["agent_context","data_context"]},"classification":{"type":"string"},"generated_from":{"type":"array","items":{"type":"string"}},"synchronization_group":{"type":["string","null"]},"remediation_kind":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"tokens":{"type":"integer"},"age_days":{"type":"integer"},"revisions_window":{"type":"integer"},"churn_pressure":{"type":"number"},"reason_codes":{"type":"array","items":{"type":"string"}},"is_pure_context_hotspot":{"type":"boolean"},"severity":{"enum":["error","warning","notice"]},"evidence_status":{"type":"string"},"next_action":{"type":"string"}}},
            "ranked_file": {"type": "object", "additionalProperties": false, "required": ["path", "slop_score", "slop_band", "context_band", "tokens", "reason_codes"], "properties":{"path":{"type":"string"},"slop_score":{"type":"number"},"slop_band":{"type":"string"},"context_band":{"type":"string"},"tokens":{"type":"integer"},"reason_codes":{"type":"array","items":{"type":"string"}}}}
        }
    });
    apply_shared_classification_schema(&mut schema);
    harden_generated_contracts(&mut schema);
    schema
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
        "anyOf":[
            {"required":["suppressed_saturated_overlays","relationship_count","structural_token_payload_omitted","analysis"]},
            {"required":["migration","evidence_limit"]}
        ],
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
        "properties":{"total":{"type":"integer","minimum":0},"returned":{"type":"integer","minimum":0},"limit":{"type":["integer","null"],"minimum":0},"truncated":{"type":"boolean"}}
    });
    schema["$defs"]["index_collection_metadata"] = json!({
        "type":"object","additionalProperties":false,"required":["files","folders"],
        "properties":{"files":{"$ref":"#/$defs/collection_page"},"folders":{"$ref":"#/$defs/collection_page"}}
    });
    schema["properties"]["collection_metadata"] = json!({
        "type":"object","additionalProperties":false,
        "required":["files","folders","compare_index","policy_index","action_queue","observation_feed","ranked_files","health.findings","health.refactor_candidates","health.watchlist"],
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
                    ("generated_from", json!([])),
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
