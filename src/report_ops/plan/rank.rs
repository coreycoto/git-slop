use super::*;

fn plan_candidates(report: &Value, context: &Value) -> Vec<Value> {
    let mut slices = vec![anchor_plan_slice(report, context)];
    let selector_kind = string(value_at(context, &["selector", "kind"]));
    let record_type = string(value_at(context, &["target", "record_type"]));
    match selector_kind.as_str() {
        "path" => {
            let relationship_limit = if record_type == "folder" {
                usize::MAX
            } else {
                3
            };
            for relationship in array_at(context, &["supporting_relationships"])
                .iter()
                .take(relationship_limit)
            {
                if let Some(slice) = relationship_plan_slice(report, context, relationship) {
                    slices.push(slice);
                }
            }
            let cluster_limit = if record_type == "folder" {
                usize::MAX
            } else {
                2
            };
            let mut emitted = 0;
            for cluster in array_at(context, &["supporting_clusters"]) {
                if let Some(slice) = cluster_plan_slice(report, context, cluster) {
                    slices.push(slice);
                    emitted += 1;
                    if emitted >= cluster_limit {
                        break;
                    }
                }
            }
        }
        "cluster" => {
            let mut emitted = 0;
            for relationship in array_at(context, &["supporting_relationships"]) {
                if let Some(slice) = relationship_plan_slice(report, context, relationship) {
                    slices.push(slice);
                    emitted += 1;
                    if emitted >= 2 {
                        break;
                    }
                }
            }
        }
        _ => {
            let mut emitted = 0;
            for cluster in array_at(context, &["supporting_clusters"]) {
                if let Some(slice) = cluster_plan_slice(report, context, cluster) {
                    slices.push(slice);
                    emitted += 1;
                    if emitted >= 2 {
                        break;
                    }
                }
            }
        }
    }
    slices
}

fn merge_plan_slices(slices: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::new();
    for mut slice in slices {
        let scope = string_array(slice.get("scope_paths"));
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| string_array(existing.get("scope_paths")) == scope)
        {
            for key in [
                "out_of_scope_paths",
                "supporting_relationship_ids",
                "supporting_cluster_ids",
            ] {
                let values = unique_strings(
                    string_array(existing.get(key))
                        .into_iter()
                        .chain(string_array(slice.get(key))),
                );
                existing
                    .as_object_mut()
                    .expect("slice object")
                    .insert(key.to_string(), json!(values));
            }
            if usize_value(slice.get("_selector_class"))
                < usize_value(existing.get("_selector_class"))
            {
                for key in [
                    "_selector_class",
                    "id",
                    "title",
                    "why_this_slice",
                    "ranking_reason",
                ] {
                    existing.as_object_mut().expect("slice object").insert(
                        key.to_string(),
                        slice
                            .as_object_mut()
                            .expect("slice object")
                            .remove(key)
                            .unwrap_or(Value::Null),
                    );
                }
            }
        } else {
            merged.push(slice);
        }
    }
    merged
}

fn plan_top_score_sum(report: &Value, slice: &Value) -> f64 {
    let mut scores: Vec<f64> = string_array(slice.get("scope_paths"))
        .iter()
        .map(|path| record_slop_score(report, path))
        .collect();
    scores.sort_by(|left, right| cmp_f64_desc(*left, *right));
    scores.into_iter().take(3).sum()
}

fn rank_plan_slices(report: &Value, context: &Value, mut slices: Vec<Value>) -> Vec<Value> {
    let path_selector = string(value_at(context, &["selector", "kind"])) == "path";
    slices.sort_by(|left, right| {
        if path_selector {
            let left_anchor = usize_value(left.get("_selector_class")) != 0;
            let right_anchor = usize_value(right.get("_selector_class")) != 0;
            left_anchor
                .cmp(&right_anchor)
                .then_with(|| {
                    cmp_f64_desc(
                        plan_top_score_sum(report, left),
                        plan_top_score_sum(report, right),
                    )
                })
                .then_with(|| {
                    string_array(left.get("out_of_scope_paths"))
                        .len()
                        .cmp(&string_array(right.get("out_of_scope_paths")).len())
                })
                .then_with(|| {
                    string_array(left.get("scope_paths"))
                        .cmp(&string_array(right.get("scope_paths")))
                })
        } else {
            usize_value(left.get("_selector_class"))
                .cmp(&usize_value(right.get("_selector_class")))
                .then_with(|| {
                    string_array(right.get("supporting_relationship_ids"))
                        .len()
                        .cmp(&string_array(left.get("supporting_relationship_ids")).len())
                })
                .then_with(|| {
                    string_array(right.get("supporting_cluster_ids"))
                        .len()
                        .cmp(&string_array(left.get("supporting_cluster_ids")).len())
                })
                .then_with(|| {
                    string_array(left.get("out_of_scope_paths"))
                        .len()
                        .cmp(&string_array(right.get("out_of_scope_paths")).len())
                })
                .then_with(|| {
                    cmp_f64_desc(
                        plan_top_score_sum(report, left),
                        plan_top_score_sum(report, right),
                    )
                })
                .then_with(|| {
                    string_array(left.get("scope_paths"))
                        .cmp(&string_array(right.get("scope_paths")))
                })
        }
    });
    let mut kept: Vec<Value> = Vec::new();
    for slice in slices {
        let scope: BTreeSet<String> = string_array(slice.get("scope_paths")).into_iter().collect();
        let relationships: BTreeSet<String> =
            string_array(slice.get("supporting_relationship_ids"))
                .into_iter()
                .collect();
        let clusters: BTreeSet<String> = string_array(slice.get("supporting_cluster_ids"))
            .into_iter()
            .collect();
        let suppress = kept.iter().any(|existing| {
            let existing_scope: BTreeSet<String> = string_array(existing.get("scope_paths"))
                .into_iter()
                .collect();
            scope.is_subset(&existing_scope)
                && scope != existing_scope
                && relationships.is_subset(
                    &string_array(existing.get("supporting_relationship_ids"))
                        .into_iter()
                        .collect(),
                )
                && clusters.is_subset(
                    &string_array(existing.get("supporting_cluster_ids"))
                        .into_iter()
                        .collect(),
                )
        });
        if !suppress {
            kept.push(slice);
        }
    }
    kept
}

fn plan_evidence_summary(slice: &Value) -> String {
    let relationships = string_array(slice.get("supporting_relationship_ids"));
    let clusters = string_array(slice.get("supporting_cluster_ids"));
    let mut parts = Vec::new();
    if !relationships.is_empty() {
        parts.push(format!("{} relationship(s)", relationships.len()));
    }
    if !clusters.is_empty() {
        parts.push(format!("{} cluster(s)", clusters.len()));
    }
    if parts.is_empty() {
        parts.push("anchor detector evidence".to_string());
    }
    format!(
        "{} Evidence: {}. Scope: {}.",
        string(slice.get("why_this_slice")),
        parts.join(", "),
        {
            let scope = string_array(slice.get("scope_paths"));
            if scope.is_empty() {
                "none".to_string()
            } else {
                scope.join(", ")
            }
        },
    )
}

fn enrich_plan_slice(report: &Value, context: &Value, mut slice: Value) -> Value {
    slice
        .as_object_mut()
        .expect("slice object")
        .remove("_selector_class");
    let evidence_summary = plan_evidence_summary(&slice);
    let top_score = string_array(slice.get("scope_paths"))
        .iter()
        .map(|path| record_slop_score(report, path))
        .fold(0.0, f64::max);
    let priority = if top_score >= 75.0 {
        "Now"
    } else if top_score >= 40.0 {
        "Next"
    } else {
        "Later"
    };
    let backlog = json!({
        "mutation_policy": "preview_only",
        "proposed_issue_title": format!("Maintenance: {}", string(slice.get("title"))),
        "issue_type": "maintenance",
        "suggested_labels": ["maintenance"],
        "priority_hint": priority,
        "evidence_summary": evidence_summary,
        "acceptance_criteria": [
            "Review the scoped paths against the cited git-slop evidence.",
            "Keep changes bounded to the proposed scope unless new evidence is documented.",
            "Preserve detector score, check, and overlay semantics.",
        ],
        "source": {
            "command": "git slop plan",
            "selector": context.get("selector").cloned().unwrap_or(Value::Null),
            "report_schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
        },
    });
    slice
        .as_object_mut()
        .expect("slice object")
        .insert("evidence_summary".to_string(), json!(evidence_summary));
    slice
        .as_object_mut()
        .expect("slice object")
        .insert("backlog_handoff".to_string(), backlog);
    slice
}

pub fn plan_payload(report: &Value, selector: PlanSelector, max_slices: usize) -> Result<Value> {
    require_report_schema(report, "plan")?;
    if max_slices == 0 {
        bail!("--max-slices must be greater than zero.");
    }
    let context = match selector {
        PlanSelector::Path(path) => plan_context_for_path(report, &path)?,
        PlanSelector::Cluster(id) => plan_context_for_cluster(report, &id)?,
        PlanSelector::Relationship(id) => plan_context_for_relationship(report, &id)?,
    };
    let candidates = plan_candidates(report, &context);
    let merged = merge_plan_slices(candidates);
    let ranked = rank_plan_slices(report, &context, merged);
    let slices: Vec<Value> = ranked
        .into_iter()
        .take(max_slices)
        .map(|slice| enrich_plan_slice(report, &context, slice))
        .collect();
    let selector_kind = string(value_at(&context, &["selector", "kind"]));
    Ok(json!({
        "schema_version": PLAN_SCHEMA_VERSION,
        "report_schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
        "command": "plan",
        "selector": context.get("selector").cloned().unwrap_or(Value::Null),
        "target": context.get("target").cloned().unwrap_or(Value::Null),
        "proposed_slices": slices,
        "ranking_basis": {
            "anchor_first": true,
            "relationship_slices_before_cluster_slices": selector_kind != "path",
            "max_slice_files": MAX_SLICE_FILES,
            "secondary_sort": if selector_kind == "path" {
                "top-three-slop-score-sum, out-of-scope-count, path"
            } else {
                "relationship-count, cluster-count, out-of-scope-count, top-three-slop-score-sum, path"
            },
        },
        "backlog_handoff": {
            "mutation_policy": "preview_only",
            "candidate_count": slices.len(),
            "target_plugin_skill": "$project-management-workflows:plan-to-backlog-preview",
            "source_selector": context.get("selector").cloned().unwrap_or(Value::Null),
        },
        "boundary_note": PLAN_BOUNDARY_NOTE,
    }))
}

fn render_limited(values: &[String], limit: usize) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    let preview = values.iter().take(limit).cloned().collect::<Vec<_>>();
    let rendered = preview.join(", ");
    if values.len() > preview.len() {
        format!("{rendered} (+{} more)", values.len() - preview.len())
    } else {
        rendered
    }
}

pub fn render_plan_text(payload: &Value) -> String {
    let target = payload.get("target").unwrap_or(&Value::Null);
    let header = match string(target.get("kind")).as_str() {
        "path" => format!(
            "Plan: path {} [{}]",
            string(target.get("path")),
            string(target.get("record_type"))
        ),
        "cluster" => format!(
            "Plan: cluster {} [{}]",
            string(target.get("id")),
            string(target.get("cluster_kind"))
        ),
        _ => format!(
            "Plan: relationship {} [{}]",
            string(target.get("id")),
            string(target.get("relationship_kind"))
        ),
    };
    let mut lines = vec![header];
    for (index, slice) in array_at(payload, &["proposed_slices"]).iter().enumerate() {
        lines.extend([
            String::new(),
            format!("{}. {}", index + 1, string(slice.get("title"))),
            format!(
                "   scope: {}",
                render_limited(&string_array(slice.get("scope_paths")), usize::MAX)
            ),
            format!("   why: {}", string(slice.get("why_this_slice"))),
            format!(
                "   evidence_summary: {}",
                string(slice.get("evidence_summary"))
            ),
            format!(
                "   evidence: relationships={}; clusters={}",
                render_limited(&string_array(slice.get("supporting_relationship_ids")), 3),
                render_limited(&string_array(slice.get("supporting_cluster_ids")), 2),
            ),
            format!(
                "   backlog: {} priority={} policy=preview_only",
                string(value_at(
                    slice,
                    &["backlog_handoff", "proposed_issue_title"]
                )),
                string(value_at(slice, &["backlog_handoff", "priority_hint"])),
            ),
            format!(
                "   out_of_scope: {}",
                render_limited(&string_array(slice.get("out_of_scope_paths")), 5)
            ),
        ]);
    }
    lines.extend([String::new(), string(payload.get("boundary_note"))]);
    lines.join("\n")
}
