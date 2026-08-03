use super::*;

mod render;

pub use render::render_explain_text;

fn build_path_explain(report: &Value, target_path: &str) -> Result<Value> {
    let target_path = normalized_path(target_path);
    let record = resolved_record(report, &target_path)
        .ok_or_else(|| anyhow!("No record found for '{target_path}'."))?;
    let record_type = string(record.get("record_type"));
    let target = json!({
        "kind": "path",
        "path": record.get("path").cloned().unwrap_or(Value::Null),
        "record_type": record_type,
        "slop_score": record.get("slop_score").cloned().unwrap_or(Value::Null),
        "slop_band": record.get("slop_band").cloned().unwrap_or(Value::Null),
        "context_band": record.get("context_band").cloned().unwrap_or(Value::Null),
        "reason_codes": record.get("reason_codes").cloned().unwrap_or_else(|| json!([])),
    });
    let mut payload = base_explain_payload(
        report,
        json!({"kind": "path", "value": target_path}),
        target,
    );
    if record_type == "folder" {
        let folder_path = string(record.get("path"));
        let descendants = descendant_records(report, &folder_path);
        let mut hotspots = descendant_hotspots(report, &folder_path, 5);
        if hotspots.is_empty() {
            hotspots = descendants.iter().take(5).cloned().collect();
        }
        let hotspot_summaries: Vec<Value> = hotspots
            .iter()
            .filter_map(|item| {
                let path = string(item.get("path"));
                resolved_record(report, &path)
            })
            .map(|item| record_summary(Some(&item)))
            .collect();
        let mut costs = record
            .get("costs")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        costs.insert(
            "descendant_hotspots".to_string(),
            Value::Array(hotspot_summaries.clone()),
        );
        let mut overlays = record
            .get("overlays")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        overlays.insert(
            "descendant_overlay_maxima".to_string(),
            descendant_overlay_maxima(&descendants),
        );
        payload
            .as_object_mut()
            .expect("payload object")
            .insert("cost_summary".to_string(), Value::Object(costs));
        payload
            .as_object_mut()
            .expect("payload object")
            .insert("overlay_summary".to_string(), Value::Object(overlays));
        let anchors: Vec<String> = hotspot_summaries
            .iter()
            .map(|item| string(item.get("path")))
            .collect();
        let anchors = if anchors.is_empty() {
            descendants
                .iter()
                .take(5)
                .map(|item| string(item.get("path")))
                .collect()
        } else {
            anchors
        };
        let relationships = rank_relationships(
            matching_relationships(report, &folder_path, true),
            &anchors,
            Some(&folder_path),
        );
        let clusters = rank_clusters(
            matching_clusters(report, &folder_path, true),
            &anchors,
            Some(&folder_path),
        );
        payload.as_object_mut().expect("payload object").insert(
            "supporting_relationships".to_string(),
            Value::Array(relationships.into_iter().take(5).collect()),
        );
        payload.as_object_mut().expect("payload object").insert(
            "supporting_clusters".to_string(),
            Value::Array(clusters.into_iter().take(5).collect()),
        );
        let summary = evidence_summary(&payload, "Folder");
        payload
            .as_object_mut()
            .expect("payload object")
            .insert("evidence_summary".to_string(), summary);
    } else {
        payload.as_object_mut().expect("payload object").insert(
            "cost_summary".to_string(),
            record.get("costs").cloned().unwrap_or_else(|| json!({})),
        );
        payload.as_object_mut().expect("payload object").insert(
            "overlay_summary".to_string(),
            record.get("overlays").cloned().unwrap_or_else(|| json!({})),
        );
        let path = string(record.get("path"));
        let anchors = vec![path.clone()];
        let relationships =
            rank_relationships(matching_relationships(report, &path, false), &anchors, None);
        let clusters = rank_clusters(matching_clusters(report, &path, false), &anchors, None);
        payload.as_object_mut().expect("payload object").insert(
            "supporting_relationships".to_string(),
            Value::Array(relationships.into_iter().take(5).collect()),
        );
        payload.as_object_mut().expect("payload object").insert(
            "supporting_clusters".to_string(),
            Value::Array(clusters.into_iter().take(5).collect()),
        );
        let summary = evidence_summary(&payload, "Path");
        payload
            .as_object_mut()
            .expect("payload object")
            .insert("evidence_summary".to_string(), summary);
    }
    Ok(payload)
}

fn build_relationship_explain(report: &Value, id: &str) -> Result<Value> {
    let relationship = relationship_by_id(report, id)
        .ok_or_else(|| anyhow!("No relationship found for '{id}'."))?;
    let source_path = string(relationship.get("source_path"));
    let target_path = string(relationship.get("target_path"));
    let source = resolved_record(report, &source_path);
    let target = resolved_record(report, &target_path);
    let shared_clusters: Vec<Value> = shared_clusters_for_relationship(report, &relationship)
        .into_iter()
        .take(5)
        .collect();
    let mut payload = base_explain_payload(
        report,
        json!({"kind": "relationship", "value": id}),
        json!({
            "kind": "relationship",
            "id": relationship.get("id").cloned().unwrap_or(Value::Null),
            "relationship_kind": relationship.get("kind").cloned().unwrap_or(Value::Null),
            "source_path": source_path,
            "target_path": target_path,
            "evidence_score": relationship.get("evidence_score").cloned().unwrap_or(Value::Null),
        }),
    );
    payload.as_object_mut().expect("payload object").insert(
        "cost_summary".to_string(),
        json!({
            "source": record_summary(source.as_ref()),
            "target": record_summary(target.as_ref()),
        }),
    );
    payload.as_object_mut().expect("payload object").insert(
        "overlay_summary".to_string(),
        json!({
            "organization_health": relationship.clone(),
            "source_overlays": source.as_ref().and_then(|value| value.get("overlays")).cloned().unwrap_or_else(|| json!({})),
            "target_overlays": target.as_ref().and_then(|value| value.get("overlays")).cloned().unwrap_or_else(|| json!({})),
        }),
    );
    payload.as_object_mut().expect("payload object").insert(
        "supporting_relationships".to_string(),
        Value::Array(vec![relationship]),
    );
    payload.as_object_mut().expect("payload object").insert(
        "supporting_clusters".to_string(),
        Value::Array(shared_clusters),
    );
    let summary = evidence_summary(&payload, "Relationship");
    payload
        .as_object_mut()
        .expect("payload object")
        .insert("evidence_summary".to_string(), summary);
    Ok(payload)
}

fn build_cluster_explain(report: &Value, id: &str) -> Result<Value> {
    let cluster =
        cluster_by_id(report, id).ok_or_else(|| anyhow!("No cluster found for '{id}'."))?;
    let members = string_array(cluster.get("member_paths"));
    let mut member_records: Vec<Value> = members
        .iter()
        .filter_map(|path| resolved_record(report, path))
        .collect();
    member_records.sort_by(|left, right| {
        cmp_f64_desc(
            number(left.get("slop_score")),
            number(right.get("slop_score")),
        )
        .then_with(|| string(left.get("path")).cmp(&string(right.get("path"))))
    });
    let relationship_ids: BTreeSet<String> = string_array(cluster.get("source_relationship_ids"))
        .into_iter()
        .collect();
    let relationships = rank_relationships(
        all_relationships(report, true)
            .into_iter()
            .filter(|item| relationship_ids.contains(&string(item.get("id"))))
            .collect(),
        &members,
        None,
    );
    let mut payload = base_explain_payload(
        report,
        json!({"kind": "cluster", "value": id}),
        json!({
            "kind": "cluster",
            "id": cluster.get("id").cloned().unwrap_or(Value::Null),
            "cluster_kind": cluster.get("kind").cloned().unwrap_or(Value::Null),
            "candidate_type": cluster.get("candidate_type").cloned().unwrap_or(Value::Null),
            "member_count": cluster.get("member_count").cloned().unwrap_or_else(|| json!(members.len())),
            "member_paths": members,
            "top_level_roots": cluster.get("top_level_roots").cloned().unwrap_or_else(|| json!([])),
        }),
    );
    payload.as_object_mut().expect("payload object").insert(
        "cost_summary".to_string(),
        json!({
            "member_hotspots": member_records.iter().take(5).map(|record| record_summary(Some(record))).collect::<Vec<_>>(),
            "member_count": cluster.get("member_count").cloned().unwrap_or_else(|| json!(member_records.len())),
            "top_level_roots": cluster.get("top_level_roots").cloned().unwrap_or_else(|| json!([])),
        }),
    );
    payload.as_object_mut().expect("payload object").insert(
        "overlay_summary".to_string(),
        json!({
            "organization_health": cluster.clone(),
            "member_overlay_maxima": descendant_overlay_maxima(&member_records),
        }),
    );
    payload.as_object_mut().expect("payload object").insert(
        "supporting_relationships".to_string(),
        Value::Array(relationships.into_iter().take(5).collect()),
    );
    payload.as_object_mut().expect("payload object").insert(
        "supporting_clusters".to_string(),
        Value::Array(vec![cluster]),
    );
    let summary = evidence_summary(&payload, "Cluster");
    payload
        .as_object_mut()
        .expect("payload object")
        .insert("evidence_summary".to_string(), summary);
    Ok(payload)
}

pub fn explain_payload(report: &Value, selector: Option<ExplainSelector>) -> Result<Value> {
    match selector.unwrap_or(ExplainSelector::Top(5)) {
        ExplainSelector::Path(path) => build_path_explain(report, &path),
        ExplainSelector::Cluster(id) => build_cluster_explain(report, &id),
        ExplainSelector::Relationship(id) => build_relationship_explain(report, &id),
        ExplainSelector::Top(count) => {
            if count == 0 {
                bail!("--top must be greater than zero.");
            }
            let mut items = Vec::new();
            for item in array_at(report, &["action_queue"]).iter().take(count) {
                items.push(build_path_explain(report, &string(item.get("path")))?);
            }
            Ok(json!({
                "schema_version": EXPLAIN_SCHEMA_VERSION,
                "report_schema_version": report_schema(report),
                "command": "explain",
                "selector": {"kind": "top", "value": count},
                "target": {"kind": "top", "count": count},
                "items": items,
                "evidence_summary": {
                    "detector_cost": ["top explanations preserve the current action_queue order"],
                    "strongest_overlays": [],
                    "supporting_evidence": {"relationship_ids": [], "cluster_ids": []},
                    "interpretation": "Top explanations describe detector ordering; they do not rerank hotspots or prove a refactor is required.",
                },
                "boundary_note": EXPLAIN_BOUNDARY_NOTE,
            }))
        }
    }
}
