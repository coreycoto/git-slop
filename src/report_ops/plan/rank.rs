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
    let round3 = |value: f64| (value * 1_000.0).round() / 1_000.0;
    let top_score = round3(top_score);
    let target_classification = string(value_at(context, &["target", "classification"]));
    let target_non_actionable = matches!(
        target_classification.as_str(),
        "generated" | "vendored" | "snapshot" | "fixture" | "migration_fixture"
    );
    let target_score: Option<f64> = None;
    let target_band = if top_score >= 75.0 {
        Some("moderate_or_lower")
    } else if top_score >= 40.0 {
        Some("low")
    } else {
        None
    };
    let non_actionable_scope = target_non_actionable
        || scope_paths.iter().any(|path| {
            resolved_record(report, path).is_some_and(|record| {
                matches!(
                    string(record.get("classification")).as_str(),
                    "generated" | "vendored" | "snapshot" | "fixture" | "migration_fixture"
                )
            })
        });
    let supported_relationship_ids = relationship_ids
        .iter()
        .filter(|id| {
            relationship_by_id(report, id)
                .is_some_and(|relationship| string(relationship.get("confidence")) == "supported")
        })
        .cloned()
        .collect::<Vec<_>>();
    let contextual_relationship_ids = relationship_ids
        .iter()
        .filter(|id| !supported_relationship_ids.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    let anchor_reason_codes = scope_paths
        .iter()
        .flat_map(|path| {
            resolved_record(report, path)
                .map(|record| string_array(record.get("reason_codes")))
                .unwrap_or_default()
        })
        .collect::<BTreeSet<_>>();
    let anchor_intervention_evidence = top_score >= 40.0 || !anchor_reason_codes.is_empty();
    let no_intervention_evidence =
        !anchor_intervention_evidence && supported_relationship_ids.is_empty();
    let plan_type = if !non_actionable_scope
        && (anchor_intervention_evidence || !supported_relationship_ids.is_empty())
    {
        "intervention"
    } else {
        "investigation"
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
                "confidence": relationship.get("confidence").cloned().unwrap_or_else(|| json!("unknown")),
                "evidence_lower_bound": relationship.get("evidence_lower_bound").or_else(|| relationship.get("confidence_lower_bound")).cloned().unwrap_or(Value::Null),
                "support_count": relationship.get("support_count").cloned().unwrap_or_else(|| json!(0)),
                "evidence_score": relationship.get("evidence_score").cloned().unwrap_or_else(|| json!(0.0)),
            })
        })
        .collect::<Vec<_>>();
    let repository_paths = array_at(report, &["files"])
        .iter()
        .filter_map(|record| record.get("path").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let configured_commands = string_array(report.pointer("/config/verification/commands"));
    let verification_commands = super::super::verification::from_report_paths(
        report,
        &repository_paths,
        &scope_paths,
        &configured_commands,
    );
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
    let acceptance_criteria = if non_actionable_scope {
        json!([
            format!(
                "Change no more than {scope_path_count} existing scoped paths unless the plan is regenerated."
            ),
            "Identify and verify the generator, upstream dependency, or fixture/test strategy without requiring a score reduction in derived content.",
            "Produce zero native compare regressions and pass every discovered verification command."
        ])
    } else if no_intervention_evidence {
        json!([
            format!(
                "Change no more than {scope_path_count} existing scoped paths unless the plan is regenerated."
            ),
            "No intervention evidence is present; gather a supported reason code, band breach, or relationship before proposing repository mutation.",
            "If evidence remains absent, close the investigation without changing source files."
        ])
    } else {
        json!([
            format!(
                "Change no more than {scope_path_count} existing scoped paths unless the plan is regenerated."
            ),
            "Remove or materially reduce at least one cited source reason code or exit the cited maintenance band while preserving raw-content identity evidence.",
            "Produce zero native compare regressions and pass every discovered verification command."
        ])
    };
    let backlog = json!({
        "mutation_policy": "preview_only",
        "proposed_issue_title": format!("Maintenance: {}", string(slice.get("title"))),
        "issue_type": "maintenance",
        "suggested_labels": ["maintenance"],
        "priority_hint": priority,
        "evidence_summary": evidence_summary,
        "acceptance_criteria": acceptance_criteria,
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
        if non_actionable_scope {
            json!(format!(
                "Investigate the generator, upstream source, or fixture/test strategy associated with {}; do not require a direct score reduction in derived content, and pass every discovered verification command without unreviewed scope expansion.",
                render_limited(&scope_paths, 5)
            ))
        } else if no_intervention_evidence {
            json!(format!(
                "Investigate {}; no intervention evidence is currently present, so do not mutate source unless a supported reason code, band breach, or relationship is established.",
                render_limited(&scope_paths, 5)
            ))
        } else {
            json!(format!(
                "Resolve the cited detector reason codes across {}, improve at least one source-derived metric without worsening supported relationship evidence, introduce zero native compare regressions, and pass every discovered verification command without unreviewed scope expansion.",
                render_limited(&scope_paths, 5)
            ))
        },
    );
    object.insert("plan_type".to_string(), json!(plan_type));
    object.insert("rationale".to_string(), json!(rationale));
    object.insert(
        "evidence".to_string(),
        json!({
            "summary": evidence_summary,
            "anchor": {
                "intervention_supported": anchor_intervention_evidence,
                "reason_codes": anchor_reason_codes,
                "top_slop_score": top_score
            },
            "relationship_support": {
                "supported_ids": supported_relationship_ids,
                "context_only_ids": contextual_relationship_ids
            },
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
        json!({
            "in_scope": scope_paths,
            "out_of_scope": out_of_scope_paths,
            "existing_path_cap": {
                "maximum": scope_path_count,
                "constraint": "Only the named in-scope existing paths may change; regenerate the plan to expand this set."
            },
            "new_path_cap": {
                "maximum": 2,
                "constraint": "Only focused tests or one extracted module directly attributable to an in-scope path; regenerate the plan for any other new file."
            }
        }),
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
            ],
            "concrete_targets": scope_paths.iter().map(|path| {
                let record = resolved_record(report, path).unwrap_or(Value::Null);
                json!({
                    "path": path,
                    "symbols_or_terms": string_array(record.get("top_structural_terms")).into_iter().take(5).collect::<Vec<_>>(),
                    "nearby_tests": record.pointer("/overlays/verification/nearby_test_paths").cloned().unwrap_or_else(|| json!([])),
                    "nearby_verification": record.pointer("/overlays/verification/nearby_verification_paths").cloned().unwrap_or_else(|| json!([]))
                })
            }).collect::<Vec<_>>()
        }),
    );
    object.insert(
        "expected_outcome".to_string(),
        json!({
            "maximum_scope_paths": scope_path_count,
            "baseline_top_slop_score": top_score,
            "target_top_slop_score": target_score,
            "target_slop_band": target_band,
            "required": if non_actionable_scope {
                json!([
                    "No new native compare regression.",
                    "The generator, upstream source, or fixture/test strategy is identified and verified; derived-content score reduction is not required.",
                    "All reviewed verification commands pass."
                ])
            } else if no_intervention_evidence {
                json!([
                    "No repository mutation without newly established intervention evidence.",
                    "The investigation records whether a reason-code, band, or supported relationship signal exists.",
                    "All reviewed verification commands pass if any diagnostic-only change is made."
                ])
            } else {
                json!([
                    "No new native compare regression.",
                    "At least one cited source-derived reason code is removed, or the scoped maintenance band improves.",
                    "Supported relationship evidence remains explainable; limited and low-support edges are investigation context only.",
                    "All reviewed verification commands pass."
                ])
            }
        }),
    );
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
    let safe = |value: Option<&Value>| crate::text::visible_controls(&string(value));
    let safe_array = |value: Option<&Value>| {
        string_array(value)
            .into_iter()
            .map(|value| crate::text::visible_controls(&value))
            .collect::<Vec<_>>()
    };
    let target = payload.get("target").unwrap_or(&Value::Null);
    let header = match string(target.get("kind")).as_str() {
        "path" => format!(
            "Plan: path {} [{}]",
            safe(target.get("path")),
            safe(target.get("record_type"))
        ),
        "cluster" => format!(
            "Plan: cluster {} [{}]",
            safe(target.get("id")),
            safe(target.get("cluster_kind"))
        ),
        _ => format!(
            "Plan: relationship {} [{}]",
            safe(target.get("id")),
            safe(target.get("relationship_kind"))
        ),
    };
    let slices = array_at(payload, &["proposed_slices"]);
    let mut lines = vec![header];
    if let Some(first) = slices.first() {
        lines.extend([
            String::new(),
            "Baseline workflow (shared by every slice):".to_string(),
            format!("  create: {}", safe(first.get("baseline_command"))),
            format!("  verify: {}", safe(first.get("rerun_command"))),
            format!(
                "  accept intentional movement: {}",
                safe(first.get("baseline_update_command"))
            ),
        ]);
    }
    for (index, slice) in slices.iter().enumerate() {
        lines.extend([
            String::new(),
            format!(
                "{}. {} [{}; priority={}]",
                index + 1,
                safe(slice.get("title")),
                safe(slice.get("plan_type")),
                safe(value_at(slice, &["backlog_handoff", "priority_hint"])),
            ),
            format!(
                "   scope: {}",
                render_limited(&safe_array(slice.get("scope_paths")), usize::MAX)
            ),
            format!("   objective: {}", safe(slice.get("objective"))),
            format!(
                "   evidence: {}",
                safe(value_at(slice, &["evidence", "summary"]))
            ),
        ]);
        let outcomes = safe_array(value_at(slice, &["expected_outcome", "required"]));
        if !outcomes.is_empty() {
            lines.push("   success:".to_string());
            lines.extend(
                outcomes
                    .into_iter()
                    .map(|outcome| format!("      - {outcome}")),
            );
        }
        let commands = safe_array(value_at(slice, &["verification", "discovered_commands"]));
        lines.push("   verification:".to_string());
        if commands.is_empty() {
            lines.push("      - none discovered; identify a focused repository-native check before editing".to_string());
        } else {
            lines.extend(
                commands
                    .into_iter()
                    .map(|command| format!("      - {command}")),
            );
        }
        let out_of_scope = safe_array(slice.get("out_of_scope_paths"));
        if !out_of_scope.is_empty() {
            lines.push(format!(
                "   explicitly out of scope: {}",
                render_limited(&out_of_scope, usize::MAX)
            ));
        }
        lines.extend([
            format!("   stop if: {}", safe(slice.get("abandonment_condition"))),
            format!(
                "   backlog preview: {}",
                safe(value_at(
                    slice,
                    &["backlog_handoff", "proposed_issue_title"]
                ))
            ),
        ]);
    }
    lines.extend([String::new(), safe(payload.get("boundary_note"))]);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_plan_preserves_known_nearby_tests() {
        let legacy: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/reports/relationship_focused_report.json"
        ))
        .expect("fixture report");
        let mut report = crate::report::migrate_legacy_report(legacy).expect("migrated report");
        let path = report
            .pointer("/files/0/path")
            .and_then(Value::as_str)
            .expect("fixture path")
            .to_string();
        let file = report["files"]
            .as_array_mut()
            .expect("files")
            .iter_mut()
            .find(|file| file.get("path").and_then(Value::as_str) == Some(path.as_str()))
            .expect("selected file");
        file["overlays"]["verification"]["nearby_test_paths"] =
            json!(["tests/report_planning_contracts.rs"]);

        let payload = plan_payload(&report, PlanSelector::Path(path), 1).expect("plan");
        assert_eq!(
            payload.pointer("/proposed_slices/0/plan_type"),
            Some(&json!("intervention"))
        );
        assert_eq!(
            payload.pointer("/proposed_slices/0/verification/concrete_targets/0/nearby_tests"),
            Some(&json!(["tests/report_planning_contracts.rs"]))
        );
    }

    #[test]
    fn generated_targets_redirect_to_the_generator_without_score_reduction() {
        let legacy: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/reports/relationship_focused_report.json"
        ))
        .expect("fixture report");
        let mut report = crate::report::migrate_legacy_report(legacy).expect("migrated report");
        let generated_path = report["files"][0]["path"]
            .as_str()
            .expect("generated path")
            .to_string();
        let generator_path = report["files"][1]["path"]
            .as_str()
            .expect("generator path")
            .to_string();
        report["files"][0]["classification"] = json!("generated");
        report["files"][0]["generated_from"] = json!([generator_path]);

        let payload = plan_payload(&report, PlanSelector::Path(generated_path), 1).expect("plan");
        let slice = &payload["proposed_slices"][0];
        assert_eq!(slice["plan_type"], "investigation");
        assert_eq!(slice["scope_paths"], report["files"][0]["generated_from"]);
        assert_eq!(
            slice["expected_outcome"]["target_top_slop_score"],
            Value::Null
        );
        assert!(
            slice["objective"]
                .as_str()
                .is_some_and(|value| value.contains("do not require a direct score reduction"))
        );
    }
}
