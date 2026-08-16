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
    let fields_with_status = |predicate: fn(&str) -> bool| {
        completeness
            .as_object()
            .into_iter()
            .flat_map(|values| values.iter())
            .filter_map(|(field, value)| {
                value
                    .as_str()
                    .filter(|status| predicate(status))
                    .map(|status| json!({"field": field, "status": status}))
            })
            .collect::<Vec<_>>()
    };
    let incomplete_fields = fields_with_status(|status| status.contains("incomplete"));
    let bounded_fields = fields_with_status(|status| status == "bounded");
    let low_support_fields = fields_with_status(|status| status == "low_support");
    let evidence_status = if !incomplete_fields.is_empty() {
        "incomplete"
    } else if !low_support_fields.is_empty() {
        "low_support"
    } else if !bounded_fields.is_empty() {
        "bounded"
    } else {
        "complete"
    };
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
            "status": evidence_status,
            "incomplete": !incomplete_fields.is_empty(),
            "incomplete_fields": incomplete_fields,
            "bounded_fields": bounded_fields,
            "low_support_fields": low_support_fields,
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
