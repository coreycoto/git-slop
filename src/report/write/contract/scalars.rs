use serde_json::{Value, json};

pub(super) fn harden_scalar_contracts(schema: &mut Value) {
    schema["$defs"]["profile"] = json!({"enum":["agent_context","data_context"]});
    schema["$defs"]["context_band"] = json!({"enum":["compact","healthy","warning","critical"]});
    schema["$defs"]["health_band"] =
        json!({"enum":["compact","healthy","warning","budget_exceeded"]});
    schema["$defs"]["slop_band"] = json!({"enum":["low","moderate","high","critical"]});
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
                    "context_band" => Some(typed("#/$defs/context_band")),
                    "health_band" => Some(typed("#/$defs/health_band")),
                    "slop_band" => Some(typed("#/$defs/slop_band")),
                    "analysis_status" => Some(typed("#/$defs/analysis_status")),
                    "evidence_status" => Some(typed("#/$defs/evidence_status")),
                    "head_sha" => Some(typed("#/$defs/sha1")),
                    "content_sha256" => Some(typed("#/$defs/digest")),
                    "content_fingerprint" => Some(typed("#/$defs/fingerprint")),
                    "worktree_state_digest"
                    | "analyzed_content_digest"
                    | "selected_path_digest"
                    | "config_digest"
                    | "analysis_config_digest"
                    | "evidence_config_digest"
                    | "policy_config_digest"
                    | "presentation_config_digest" => Some(typed("#/$defs/digest")),
                    "slop_score" => Some(json!({"type":"number","minimum":0,"maximum":100})),
                    "context_pressure"
                    | "churn_pressure"
                    | "load_pressure"
                    | "volatility_pressure"
                    | "coordination_pressure"
                    | "top_file_share"
                    | "top_3_file_share"
                    | "token_concentration_ratio"
                    | "top_author_share"
                    | "late_churn_spike"
                    | "cochange_centrality"
                    | "cochange_pagerank"
                    | "cross_folder_cochange_ratio"
                    | "change_diffusion"
                    | "evidence_score"
                    | "similarity"
                    | "similarity_ratio"
                    | "jaccard"
                    | "calibrated_jaccard"
                    | "evidence_lower_bound"
                    | "confidence_lower_bound" => {
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
