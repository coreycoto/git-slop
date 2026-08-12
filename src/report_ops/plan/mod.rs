use super::*;

mod rank;
pub use rank::{plan_payload, render_plan_text};

fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn plan_slug(value: &str) -> String {
    value
        .replace('/', "--")
        .replace(['.', ':'], "_")
        .replace(' ', "-")
}

fn record_slop_score(report: &Value, path: &str) -> f64 {
    resolved_record(report, path)
        .as_ref()
        .map(|record| number(record.get("slop_score")))
        .unwrap_or(0.0)
}

fn sort_scope_paths(report: &Value, paths: Vec<String>, anchors: &[String]) -> Vec<String> {
    let anchor_order: BTreeMap<&str, usize> = anchors
        .iter()
        .enumerate()
        .map(|(index, path)| (path.as_str(), index))
        .collect();
    let mut records: Vec<(String, f64)> = unique_strings(paths)
        .into_iter()
        .filter_map(|path| {
            resolved_record(report, &path).map(|record| (path, number(record.get("slop_score"))))
        })
        .collect();
    records.sort_by(|left, right| {
        match (
            anchor_order.get(left.0.as_str()),
            anchor_order.get(right.0.as_str()),
        ) {
            (Some(left_index), Some(right_index)) => left_index.cmp(right_index),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => cmp_f64_desc(left.1, right.1).then_with(|| left.0.cmp(&right.0)),
        }
    });
    records.into_iter().map(|(path, _)| path).collect()
}

fn build_scope(
    report: &Value,
    candidate_paths: Vec<String>,
    anchors: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut ordered = sort_scope_paths(report, candidate_paths, anchors);
    let out_of_scope = if ordered.len() > MAX_SLICE_FILES {
        ordered.split_off(MAX_SLICE_FILES)
    } else {
        Vec::new()
    };
    (ordered, out_of_scope)
}

fn folder_anchor_paths(report: &Value, folder: &str) -> Vec<String> {
    let queued: Vec<String> = descendant_hotspots(report, folder, MAX_SLICE_FILES)
        .iter()
        .map(|item| string(item.get("path")))
        .collect();
    if !queued.is_empty() {
        queued
    } else {
        descendant_records(report, folder)
            .iter()
            .take(MAX_SLICE_FILES)
            .map(|item| string(item.get("path")))
            .collect()
    }
}

fn plan_path_target(record: &Value) -> Value {
    json!({
        "kind": "path",
        "path": record.get("path").cloned().unwrap_or(Value::Null),
        "record_type": record.get("record_type").cloned().unwrap_or(Value::Null),
        "slop_score": record.get("slop_score").cloned().unwrap_or(Value::Null),
        "slop_band": record.get("slop_band").cloned().unwrap_or(Value::Null),
        "context_band": record.get("context_band").cloned().unwrap_or(Value::Null),
        "classification": record.get("classification").cloned().unwrap_or(Value::Null),
        "generated_from": record.get("generated_from").cloned().unwrap_or_else(|| json!([])),
        "generated_provenance": record.get("generated_provenance").cloned().unwrap_or_else(|| json!({})),
        "reason_codes": record.get("reason_codes").cloned().unwrap_or_else(|| json!([])),
    })
}

fn plan_cluster_target(cluster: &Value) -> Value {
    let members = string_array(cluster.get("member_paths"));
    json!({
        "kind": "cluster",
        "id": cluster.get("id").cloned().unwrap_or(Value::Null),
        "cluster_kind": cluster.get("kind").cloned().unwrap_or(Value::Null),
        "candidate_type": cluster.get("candidate_type").cloned().unwrap_or(Value::Null),
        "member_count": cluster.get("member_count").cloned().unwrap_or_else(|| json!(members.len())),
        "member_paths": members,
        "top_level_roots": cluster.get("top_level_roots").cloned().unwrap_or_else(|| json!([])),
    })
}

fn plan_relationship_target(relationship: &Value) -> Value {
    json!({
        "kind": "relationship",
        "id": relationship.get("id").cloned().unwrap_or(Value::Null),
        "relationship_kind": relationship.get("kind").cloned().unwrap_or(Value::Null),
        "source_path": relationship.get("source_path").cloned().unwrap_or(Value::Null),
        "target_path": relationship.get("target_path").cloned().unwrap_or(Value::Null),
        "evidence_score": relationship.get("evidence_score").cloned().unwrap_or(Value::Null),
    })
}

fn plan_context_for_path(report: &Value, requested: &str) -> Result<Value> {
    let record = resolved_record(report, requested)
        .ok_or_else(|| anyhow!("No record found for '{}'.", normalized_path(requested)))?;
    let path = string(record.get("path"));
    let is_folder = string(record.get("record_type")) == "folder";
    let anchors = if is_folder {
        folder_anchor_paths(report, &path)
    } else if string(record.get("classification")) == "generated" {
        let generated_from = string_array(record.get("generated_from"))
            .into_iter()
            .filter(|candidate| resolved_record(report, candidate).is_some())
            .collect::<Vec<_>>();
        if generated_from.is_empty() {
            vec![path.clone()]
        } else {
            generated_from
        }
    } else {
        vec![path.clone()]
    };
    let focus = is_folder.then(|| path.clone());
    let relationships = rank_relationships(
        matching_relationships(report, &path, is_folder),
        &anchors,
        focus.as_deref(),
    );
    let clusters = rank_clusters(
        matching_clusters(report, &path, is_folder),
        &anchors,
        focus.as_deref(),
    );
    Ok(json!({
        "selector": {"kind": "path", "value": requested},
        "target": plan_path_target(&record),
        "anchor_paths": anchors,
        "supporting_relationships": relationships,
        "supporting_clusters": clusters,
        "focus_folder": focus,
        "record": record,
    }))
}

fn plan_context_for_cluster(report: &Value, id: &str) -> Result<Value> {
    let cluster =
        cluster_by_id(report, id).ok_or_else(|| anyhow!("No cluster found for '{id}'."))?;
    let members = string_array(cluster.get("member_paths"));
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
    Ok(json!({
        "selector": {"kind": "cluster", "value": id},
        "target": plan_cluster_target(&cluster),
        "anchor_paths": members,
        "supporting_relationships": relationships,
        "supporting_clusters": [cluster.clone()],
        "focus_folder": Value::Null,
        "cluster": cluster,
    }))
}

fn plan_context_for_relationship(report: &Value, id: &str) -> Result<Value> {
    let relationship = relationship_by_id(report, id)
        .ok_or_else(|| anyhow!("No relationship found for '{id}'."))?;
    let anchors = vec![
        string(relationship.get("source_path")),
        string(relationship.get("target_path")),
    ];
    let clusters = rank_clusters(
        shared_clusters_for_relationship(report, &relationship),
        &anchors,
        None,
    );
    Ok(json!({
        "selector": {"kind": "relationship", "value": id},
        "target": plan_relationship_target(&relationship),
        "anchor_paths": anchors,
        "supporting_relationships": [relationship.clone()],
        "supporting_clusters": clusters,
        "focus_folder": Value::Null,
        "relationship": relationship,
    }))
}

fn root_for_path(path: &str) -> &str {
    path.split('/').next().unwrap_or(".")
}

fn cluster_anchor_candidates(report: &Value, context: &Value) -> Vec<String> {
    let anchors = string_array(context.get("anchor_paths"));
    if string(value_at(context, &["selector", "kind"])) != "cluster" {
        return anchors;
    }
    let target = context.get("target").unwrap_or(&Value::Null);
    if usize_value(target.get("member_count")) <= MAX_SLICE_FILES
        && string_array(target.get("top_level_roots")).len() <= 2
    {
        return anchors;
    }
    if let Some(relationship) = array_at(context, &["supporting_relationships"]).first() {
        return vec![
            string(relationship.get("source_path")),
            string(relationship.get("target_path")),
        ];
    }
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &anchors {
        if resolved_record(report, path).is_some() {
            groups
                .entry(root_for_path(path).to_string())
                .or_default()
                .push(path.clone());
        }
    }
    for paths in groups.values_mut() {
        *paths = sort_scope_paths(report, std::mem::take(paths), &[]);
    }
    let mut candidates: Vec<(String, Vec<String>)> = groups
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .collect();
    if candidates.is_empty() {
        let mut fallback: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in &anchors {
            if resolved_record(report, path).is_some() {
                fallback
                    .entry(root_for_path(path).to_string())
                    .or_default()
                    .push(path.clone());
            }
        }
        candidates = fallback.into_iter().collect();
    }
    candidates.sort_by(|left, right| {
        let score = |paths: &[String]| {
            paths
                .iter()
                .take(2)
                .map(|path| record_slop_score(report, path))
                .sum::<f64>()
        };
        cmp_f64_desc(score(&left.1), score(&right.1))
            .then_with(|| {
                let left_top = left
                    .1
                    .first()
                    .map(|path| record_slop_score(report, path))
                    .unwrap_or(0.0);
                let right_top = right
                    .1
                    .first()
                    .map(|path| record_slop_score(report, path))
                    .unwrap_or(0.0);
                cmp_f64_desc(left_top, right_top)
            })
            .then_with(|| left.0.cmp(&right.0))
    });
    candidates
        .into_iter()
        .next()
        .map(|(_, paths)| paths.into_iter().take(3).collect())
        .unwrap_or_else(|| anchors.into_iter().take(3).collect())
}

#[allow(clippy::too_many_arguments)]
fn plan_slice(
    id: String,
    title: String,
    scope_paths: Vec<String>,
    out_of_scope_paths: Vec<String>,
    relationship_ids: Vec<String>,
    cluster_ids: Vec<String>,
    why: &str,
    ranking_reason: &str,
    selector_class: usize,
) -> Value {
    json!({
        "id": id,
        "title": title,
        "scope_paths": scope_paths,
        "out_of_scope_paths": out_of_scope_paths,
        "supporting_relationship_ids": relationship_ids,
        "supporting_cluster_ids": cluster_ids,
        "why_this_slice": why,
        "ranking_reason": ranking_reason,
        "_selector_class": selector_class,
    })
}

fn anchor_plan_slice(report: &Value, context: &Value) -> Value {
    let selector_kind = string(value_at(context, &["selector", "kind"]));
    let selector_value = string(value_at(context, &["selector", "value"]));
    let target = context.get("target").unwrap_or(&Value::Null);
    let candidate_paths = cluster_anchor_candidates(report, context);
    let (scope, out_of_scope) = build_scope(report, candidate_paths.clone(), &candidate_paths);
    let (title, why) = match selector_kind.as_str() {
        "path" if string(target.get("record_type")) == "folder" => (
            format!(
                "Focus descendant hotspots in {}",
                string(target.get("path"))
            ),
            "Start with the highest-ranked descendant hotspots already driving this folder's context cost.",
        ),
        "path" if string(target.get("classification")) == "generated" => (
            format!("Inspect generator for {}", string(target.get("path"))),
            "Review the generator source and synchronization contract; the generated output is evidence, not the intervention target.",
        ),
        "path"
            if matches!(
                string(target.get("classification")).as_str(),
                "snapshot" | "fixture" | "migration_fixture"
            ) =>
        {
            (
                format!(
                    "Inspect fixture strategy for {}",
                    string(target.get("path"))
                ),
                "Review the fixture generator or test strategy; score reduction in the fixture itself is not an acceptance criterion.",
            )
        }
        "path" => (
            format!("Anchor hotspot {}", string(target.get("path"))),
            "Start with the selected hotspot before expanding to adjacent structural evidence.",
        ),
        "cluster" if candidate_paths != string_array(context.get("anchor_paths")) => (
            format!("Start inside cluster {}", string(target.get("id"))),
            "The selected cluster is broad, so start with the strongest reviewable sub-slice before expanding.",
        ),
        "cluster" => (
            format!("Inspect cluster {}", string(target.get("id"))),
            "Start with the selected cluster members before splitting work into narrower relationship-driven slices.",
        ),
        _ => (
            format!(
                "Inspect {} ↔ {}",
                string(target.get("source_path")),
                string(target.get("target_path"))
            ),
            "Start with the selected coupled pair before considering any surrounding cluster evidence.",
        ),
    };
    plan_slice(
        format!("anchor-{selector_kind}-{}", plan_slug(&selector_value)),
        title,
        scope,
        out_of_scope,
        array_at(context, &["supporting_relationships"])
            .iter()
            .take(3)
            .map(|item| string(item.get("id")))
            .collect(),
        array_at(context, &["supporting_clusters"])
            .iter()
            .take(3)
            .map(|item| string(item.get("id")))
            .collect(),
        why,
        "Anchor slice always ranks first.",
        0,
    )
}

fn relationship_plan_slice(report: &Value, context: &Value, relationship: &Value) -> Option<Value> {
    let focus = optional_string(context.get("focus_folder"));
    let mut candidates = vec![
        string(relationship.get("source_path")),
        string(relationship.get("target_path")),
    ];
    if let Some(folder) = focus.as_deref() {
        let mut descendants: Vec<String> = candidates
            .iter()
            .filter(|path| path_matches_folder(path, folder))
            .cloned()
            .collect();
        let external: Vec<String> = candidates
            .into_iter()
            .filter(|path| !path_matches_folder(path, folder))
            .collect();
        if descendants.is_empty() {
            return None;
        }
        descendants.extend(external.into_iter().take(1));
        candidates = descendants;
    }
    let anchors = string_array(context.get("anchor_paths"));
    let (scope, out_of_scope) = build_scope(report, candidates, &anchors);
    if scope.len() < 2 {
        return None;
    }
    if let Some(folder) = focus.as_deref() {
        let descendants = scope
            .iter()
            .filter(|path| path_matches_folder(path, folder))
            .count();
        let external = scope.len() - descendants;
        if !(descendants >= 2 || (descendants == 1 && external >= 1)) {
            return None;
        }
    }
    let id = string(relationship.get("id"));
    let clusters = shared_clusters_for_relationship(report, relationship)
        .into_iter()
        .take(3)
        .map(|item| string(item.get("id")))
        .collect();
    Some(plan_slice(
        format!("relationship-{id}"),
        format!(
            "Inspect {} ↔ {}",
            string(relationship.get("source_path")),
            string(relationship.get("target_path"))
        ),
        scope,
        out_of_scope,
        vec![id],
        clusters,
        "This pair already co-occurs in direct detector evidence and should be reviewed together.",
        "Direct relationship slices rank immediately after the anchor slice.",
        1,
    ))
}

fn cluster_scope_candidates(report: &Value, context: &Value, cluster: &Value) -> Vec<String> {
    let members = string_array(cluster.get("member_paths"));
    let Some(folder) = optional_string(context.get("focus_folder")) else {
        return members;
    };
    let descendants: Vec<String> = members
        .iter()
        .filter(|path| path_matches_folder(path, &folder))
        .cloned()
        .collect();
    let external: Vec<String> = members
        .into_iter()
        .filter(|path| !path_matches_folder(path, &folder))
        .collect();
    let mut candidates = descendants;
    candidates.extend(sort_scope_paths(report, external, &[]).into_iter().take(1));
    candidates
}

fn compact_cluster(cluster: &Value) -> bool {
    matches!(
        string(cluster.get("kind")).as_str(),
        "duplicate_set" | "consolidation_candidate"
    ) || matches!(
        string(cluster.get("candidate_type")).as_str(),
        "duplicate_set" | "consolidation_candidate" | "consolidate_duplicate_knowledge"
    )
}

fn cluster_plan_slice(report: &Value, context: &Value, cluster: &Value) -> Option<Value> {
    let candidates = cluster_scope_candidates(report, context, cluster);
    let anchors = string_array(context.get("anchor_paths"));
    let (scope, mut out_of_scope) = build_scope(report, candidates, &anchors);
    out_of_scope = unique_strings(
        out_of_scope.into_iter().chain(
            string_array(cluster.get("member_paths"))
                .into_iter()
                .filter(|path| !scope.contains(path)),
        ),
    );
    let focus = optional_string(context.get("focus_folder"));
    let descendant_count = focus
        .as_deref()
        .map(|folder| {
            scope
                .iter()
                .filter(|path| path_matches_folder(path, folder))
                .count()
        })
        .unwrap_or(0);
    if focus.is_some() {
        let external = scope.len() - descendant_count;
        if !(descendant_count >= 2 || (descendant_count == 1 && external >= 1)) {
            return None;
        }
    }
    let member_count = usize_value(cluster.get("member_count")).max(1);
    let selector_kind = string(value_at(context, &["selector", "kind"]));
    let qualifies = member_count <= 8
        || scope.len() as f64 / member_count as f64 >= 0.5
        || compact_cluster(cluster)
        || (selector_kind == "path" && descendant_count >= 2);
    if !qualifies {
        return None;
    }
    if selector_kind == "path"
        && string(value_at(context, &["target", "record_type"])) == "file"
        && !scope.iter().any(|path| !anchors.contains(path))
    {
        return None;
    }
    if selector_kind == "relationship"
        && (!scope.iter().any(|path| !anchors.contains(path)) || out_of_scope.len() > scope.len())
    {
        return None;
    }
    let id = string(cluster.get("id"));
    let selector_class =
        if (member_count <= 8 || compact_cluster(cluster)) && out_of_scope.is_empty() {
            2
        } else {
            3
        };
    Some(plan_slice(
        format!("cluster-{id}"),
        format!("Inspect cluster {id}"),
        scope,
        out_of_scope,
        string_array(cluster.get("source_relationship_ids"))
            .into_iter()
            .take(3)
            .collect(),
        vec![id],
        "This slice stays inside one direct structural cluster instead of sweeping a broader folder.",
        "Cluster slices rank after direct relationship slices.",
        selector_class,
    ))
}
