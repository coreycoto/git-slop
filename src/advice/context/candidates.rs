fn sha256(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

fn canonical_digest(value: &Value) -> Result<String> {
    Ok(sha256(serde_json::to_vec(value)?))
}

fn context_digest(value: &Value) -> Result<String> {
    let mut normalized = value.clone();
    if let Some(object) = normalized.as_object_mut() {
        object.remove("context_digest");
    }
    if let Some(limits) = normalized.get_mut("limits").and_then(Value::as_object_mut) {
        limits.remove("estimated_context_tokens");
    }
    canonical_digest(&normalized)
}

fn push_strings(value: Option<&Value>, target: &mut BTreeSet<String>) {
    if let Some(values) = value.and_then(Value::as_array) {
        for value in values.iter().filter_map(Value::as_str) {
            target.insert(value.to_string());
        }
    }
}

fn plan_payloads(
    report: &Value,
    selector: &AdviceSelector,
    max_slices: usize,
) -> Result<Vec<Value>> {
    match selector {
        AdviceSelector::Path(path) => Ok(vec![plan_payload(
            report,
            PlanSelector::Path(path.clone()),
            max_slices,
        )?]),
        AdviceSelector::Relationship(id) => Ok(vec![plan_payload(
            report,
            PlanSelector::Relationship(id.clone()),
            max_slices,
        )?]),
        AdviceSelector::Cluster(id) => Ok(vec![plan_payload(
            report,
            PlanSelector::Cluster(id.clone()),
            max_slices,
        )?]),
        AdviceSelector::Top(count) => {
            if *count == 0 || *count > 20 {
                bail!("--top must be between 1 and 20");
            }
            let explanations = explain_payload(report, Some(ExplainSelector::Top(*count)))?;
            let mut paths = explanations
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| item.pointer("/target/path").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>();
            if paths.len() < *count {
                for path in report
                    .pointer("/health/refactor_candidates")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item.get("path").and_then(Value::as_str))
                {
                    if !paths.iter().any(|existing| existing == path) {
                        paths.push(path.to_string());
                    }
                    if paths.len() == *count {
                        break;
                    }
                }
            }
            paths
                .into_iter()
                .map(|path| plan_payload(report, PlanSelector::Path(path.to_string()), 1))
                .collect()
        }
    }
}

fn build_candidates(plans: &[Value]) -> Result<Vec<Value>> {
    let mut candidates = Vec::new();
    for plan in plans {
        let selector = plan.get("selector").cloned().unwrap_or(Value::Null);
        for slice in plan
            .get("proposed_slices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let source_id = slice
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unnamed-plan-slice");
            let candidate_digest = canonical_digest(&json!({
                "selector": selector,
                "source_plan_slice": slice,
            }))?;
            let candidate_id = format!("candidate-{}", &candidate_digest[..16]);
            let boundaries = slice.get("boundaries").unwrap_or(&Value::Null);
            let evidence = slice.get("evidence").unwrap_or(&Value::Null);
            let outcome = slice.get("expected_outcome").unwrap_or(&Value::Null);
            let verification = slice.get("verification").unwrap_or(&Value::Null);
            let compact_targets = verification
                .get("concrete_targets")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|target| {
                    let bounded_paths = |key: &str| {
                        target
                            .get(key)
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .take(1)
                            .cloned()
                            .collect::<Vec<_>>()
                    };
                    json!({
                        "path": target.get("path").cloned().unwrap_or(Value::Null),
                        "nearby_tests": bounded_paths("nearby_tests"),
                        "nearby_verification": bounded_paths("nearby_verification"),
                    })
                })
                .collect::<Vec<_>>();
            let assumptions = slice
                .get("assumptions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|assumption| {
                    !matches!(
                        assumption.as_str(),
                        Some(
                            "The cited detector report is the source of truth for scope and ranking."
                                | "A human reviews the proposed slice before any repository mutation."
                        )
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            let relationship_support = evidence
                .get("relationship_support")
                .and_then(Value::as_object)
                .map(|support| {
                    let supported = support
                        .get("supported_ids")
                        .and_then(Value::as_array)
                        .is_some_and(|ids| !ids.is_empty());
                    let context_only = support
                        .get("context_only_ids")
                        .and_then(Value::as_array)
                        .is_some_and(|ids| !ids.is_empty());
                    match (supported, context_only) {
                        (true, true) => "mixed",
                        (true, false) => "supported",
                        (false, true) => "context_only",
                        (false, false) => "unavailable",
                    }
                });
            candidates.push(json!({
                "id": candidate_id,
                "source_plan_slice_id": source_id,
                "observed_facts": {
                    "scope_paths": slice.get("scope_paths").cloned().unwrap_or_else(|| json!([])),
                    "out_of_scope_paths": slice.get("out_of_scope_paths").cloned().unwrap_or_else(|| json!([])),
                    "boundaries": {
                        "maximum_existing_paths": boundaries.pointer("/existing_path_cap/maximum").cloned().unwrap_or(Value::Null),
                        "maximum_new_paths": boundaries.pointer("/new_path_cap/maximum").cloned().unwrap_or(Value::Null),
                    },
                    "evidence": {
                        "anchor": evidence.get("anchor").cloned().unwrap_or(Value::Null),
                        "finding_ids": evidence.get("finding_ids").cloned().unwrap_or_else(|| json!([])),
                        "relationship_ids": evidence.get("relationship_ids").cloned().unwrap_or_else(|| json!([])),
                        "cluster_ids": evidence.get("cluster_ids").cloned().unwrap_or_else(|| json!([])),
                        "relationship_support": relationship_support,
                    },
                    "verification": {
                        "classes": verification.get("classes").cloned().unwrap_or_else(|| json!([])),
                        "concrete_targets": compact_targets,
                        "discovered_commands": verification.get("discovered_commands").cloned().unwrap_or_else(|| json!([])),
                        "required_checks": verification.get("required_checks").cloned().unwrap_or_else(|| json!([])),
                    },
                    "expected_outcome": {
                        "required": outcome.get("required").cloned().unwrap_or_else(|| json!([])),
                        "target_slop_band": outcome.get("target_slop_band").cloned().unwrap_or(Value::Null),
                        "target_top_slop_score": outcome.get("target_top_slop_score").cloned().unwrap_or(Value::Null),
                    },
                },
                "interpretation": {
                    "title": slice.get("title").cloned().unwrap_or(Value::Null),
                    "objective": "Apply and verify the bounded slice.",
                    "rationale": slice.get("rationale").cloned().unwrap_or(Value::Null),
                    "assumptions": assumptions,
                    "abandonment_condition": "Abstain if evidence is insufficient.",
                    "rollback": "Revert the bounded change.",
                },
                "implementation_sequence": [
                    "baseline",
                    "change",
                    "verify",
                    "compare"
                ],
            }));
        }
    }
    candidates.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| {
        candidate
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| seen.insert(id.to_string()))
    });
    if candidates.is_empty() {
        bail!("the selected report evidence produced no deterministic plan candidates");
    }
    Ok(candidates)
}

fn collect_candidate_paths(candidates: &[Value]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut sources = BTreeSet::new();
    let mut tests = BTreeSet::new();
    for candidate in candidates {
        let facts = candidate.get("observed_facts").unwrap_or(&Value::Null);
        push_strings(facts.get("scope_paths"), &mut sources);
        if let Some(targets) = facts
            .pointer("/verification/concrete_targets")
            .and_then(Value::as_array)
        {
            for target in targets {
                if let Some(path) = target.get("path").and_then(Value::as_str) {
                    sources.insert(path.to_string());
                }
                push_strings(target.get("nearby_tests"), &mut tests);
                push_strings(target.get("nearby_verification"), &mut tests);
            }
        }
    }
    (sources, tests)
}

fn guidance_candidates(source_paths: &BTreeSet<String>) -> Vec<String> {
    let mut guidance = Vec::new();
    for source in source_paths {
        let mut parent = Path::new(source).parent();
        while let Some(directory) = parent {
            if directory.as_os_str().is_empty() {
                break;
            }
            let path = directory
                .join("AGENTS.md")
                .to_string_lossy()
                .replace('\\', "/");
            if !guidance.contains(&path) {
                guidance.push(path);
            }
            parent = directory.parent();
        }
    }
    for path in [
        "AGENTS.md",
        "CONTRIBUTING.md",
        "README.md",
        "docs/architecture.md",
    ] {
        if !guidance.iter().any(|candidate| candidate == path) {
            guidance.push(path.to_string());
        }
    }
    guidance
}
