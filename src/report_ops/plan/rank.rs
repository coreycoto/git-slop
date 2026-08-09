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
    let rationale = string(slice.get("why_this_slice"));
    let scope_paths = string_array(slice.get("scope_paths"));
    let scope_path_count = scope_paths.len();
    let out_of_scope_paths = string_array(slice.get("out_of_scope_paths"));
    let relationship_ids = string_array(slice.get("supporting_relationship_ids"));
    let cluster_ids = string_array(slice.get("supporting_cluster_ids"));
    let baseline_command = "cp .slop/latest/report.json .slop/plan-baseline.json";
    let rerun_command = "git-slop find && git-slop compare --base .slop/plan-baseline.json --head .slop/latest/report.json --detail summary --fail-on-regression";
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
    let relationship_labels = relationship_ids
        .iter()
        .filter_map(|id| relationship_by_id(report, id))
        .map(|relationship| {
            json!({
                "id": relationship.get("id").cloned().unwrap_or(Value::Null),
                "kind": relationship.get("kind").cloned().unwrap_or(Value::Null),
                "paths": [
                    relationship.get("source_path").cloned().unwrap_or(Value::Null),
                    relationship.get("target_path").cloned().unwrap_or(Value::Null),
                ],
            })
        })
        .collect::<Vec<_>>();
    let repository_paths = array_at(report, &["files"])
        .iter()
        .filter_map(|record| record.get("path").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let verification_commands = if repository_paths.contains("Cargo.toml") {
        vec![
            "cargo fmt --all -- --check",
            "cargo clippy --all-targets --all-features -- -D warnings",
            "cargo test --all-targets",
        ]
    } else if repository_paths.contains("go.mod") {
        vec!["go test ./..."]
    } else if repository_paths.contains("pyproject.toml") {
        vec!["pytest"]
    } else if repository_paths.contains("package.json") {
        vec!["npm test"]
    } else {
        Vec::new()
    };
    let verification_classes = scope_paths
        .iter()
        .filter_map(|path| {
            array_at(report, &["files"])
                .iter()
                .find(|record| record.get("path").and_then(Value::as_str) == Some(path))
        })
        .filter_map(|record| record.get("classification").and_then(Value::as_str))
        .map(|classification| match classification {
            "docs" => "documentation",
            "config" => "configuration",
            "workflow" => "workflow",
            "tool" => "workflow_or_tooling",
            "test" => "test",
            _ => "source",
        })
        .collect::<BTreeSet<_>>();
    let backlog = json!({
        "mutation_policy": "preview_only",
        "proposed_issue_title": format!("Maintenance: {}", string(slice.get("title"))),
        "issue_type": "maintenance",
        "suggested_labels": ["maintenance"],
        "priority_hint": priority,
        "evidence_summary": evidence_summary,
        "acceptance_criteria": [
            format!("Change no more than {scope_path_count} scoped paths unless the plan is regenerated."),
            format!("Keep the highest scoped slop score at or below {top_score:.6}."),
            "Produce zero native compare regressions and pass every discovered verification command.",
        ],
        "source": {
            "command": "git slop plan",
            "selector": context.get("selector").cloned().unwrap_or(Value::Null),
            "report_schema_version": report.get("schema_version").cloned().unwrap_or(Value::Null),
        },
    });
    let object = slice.as_object_mut().expect("slice object");
    object.remove("why_this_slice");
    object.insert(
        "objective".to_string(),
        json!(format!(
            "Keep the highest scoped slop score at or below {top_score:.6} across {}, introduce zero native compare regressions, and pass every discovered verification command without expanding the reviewed scope.",
            render_limited(&scope_paths, 5)
        )),
    );
    object.insert("rationale".to_string(), json!(rationale));
    object.insert(
        "evidence".to_string(),
        json!({
            "summary": evidence_summary,
            "relationship_ids": relationship_ids,
            "relationships": relationship_labels,
            "cluster_ids": cluster_ids,
        }),
    );
    object.insert(
        "assumptions".to_string(),
        json!([
            "The cited detector report is the source of truth for scope and ranking.",
            "A human reviews the proposed slice before any repository mutation.",
            if verification_commands.is_empty() { "No repository-native verification command was discoverable from tracked manifest paths." } else { "Discovered verification commands are inferred from tracked manifest paths and must be reviewed before execution." },
        ]),
    );
    object.insert(
        "boundaries".to_string(),
        json!({"in_scope": scope_paths, "out_of_scope": out_of_scope_paths}),
    );
    object.insert(
        "verification".to_string(),
        json!({
            "classes": verification_classes,
            "discovered_commands": verification_commands,
            "required_checks": [
                "Run focused tests for every changed path.",
                "Rerun git-slop and compare the new report with the saved baseline.",
                "Confirm no unrelated finding or public contract regressed.",
            ]
        }),
    );
    object.insert(
        "expected_outcome".to_string(),
        json!({
            "maximum_scope_paths": scope_path_count,
            "baseline_top_slop_score": top_score,
            "required": [
                "No new native compare regression.",
                "No increase in the highest scoped slop score.",
                "All reviewed verification commands pass."
            ]
        }),
    );
    object.insert("baseline_command".to_string(), json!(baseline_command));
    object.insert("rerun_command".to_string(), json!(rerun_command));
    object.insert(
        "abandonment_condition".to_string(),
        json!("Stop and abandon or re-scope this slice if preserving behavior requires an out-of-scope change, verification cannot be identified, or the native comparison reports a regression."),
    );
    object.insert(
        "rollback".to_string(),
        json!("Revert only the explicitly reviewed code changes; report and prompt artifacts are advisory and can be regenerated."),
    );
    object.insert("backlog_handoff".to_string(), backlog);
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
            "source_selector": context.get("selector").cloned().unwrap_or(Value::Null),
            "canonical_format": "provider_neutral_maintenance_plan",
            "optional_adapters": ["github_issues", "linear", "project_management_plugin"],
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
            format!("   objective: {}", string(slice.get("objective"))),
            format!("   rationale: {}", string(slice.get("rationale"))),
            format!(
                "   evidence: {}",
                string(value_at(slice, &["evidence", "summary"]))
            ),
            format!(
                "   expected_outcome: highest scoped score does not exceed {}; no native compare regression; verification passes",
                json_scalar_text(value_at(slice, &["expected_outcome", "baseline_top_slop_score"]))
            ),
            format!(
                "   verification: {}",
                render_limited(
                    &string_array(value_at(slice, &["verification", "discovered_commands"])),
                    5
                )
            ),
            format!("   rerun: {}", string(slice.get("rerun_command"))),
            format!(
                "   abandon_if: {}",
                string(slice.get("abandonment_condition"))
            ),
            format!("   rollback: {}", string(slice.get("rollback"))),
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
