pub fn load_and_validate_artifact(path: &Path, report: &Value) -> Result<Value> {
    let bytes = super::io::read_bounded(
        path,
        super::io::MAX_ADVICE_ARTIFACT_BYTES,
        "advice artifact",
    )?;
    let artifact: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("unable to parse advice artifact {}", path.display()))?;
    validate_artifact_contract(&artifact)?;
    validate_artifact_semantics(&artifact)?;
    let current = sha256(serde_json::to_vec(report)?);
    let recorded = artifact
        .pointer("/report/canonical_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if current != recorded {
        bail!(
            "stale advice artifact: recorded report digest {recorded}, current selected report digest {current}"
        );
    }
    Ok(artifact)
}

fn validate_artifact_contract(artifact: &Value) -> Result<()> {
    let schema: Value = serde_json::from_str(include_str!("../../../schemas/advice-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .context("embedded advice artifact schema is invalid")?;
    if let Some(error) = validator.iter_errors(artifact).next() {
        bail!(
            "advice artifact does not match schema v{ADVICE_SCHEMA_VERSION} at {}: {}",
            error.instance_path(),
            error
        );
    }
    if artifact.get("schema_version").and_then(Value::as_u64) != Some(ADVICE_SCHEMA_VERSION)
        || artifact
            .pointer("/validation/status")
            .and_then(Value::as_str)
            != Some("valid")
    {
        bail!("advice artifact is not a validated schema-{ADVICE_SCHEMA_VERSION} artifact");
    }
    Ok(())
}

fn artifact_verdict_rank(value: &str) -> Option<u8> {
    match value {
        "approve" => Some(0),
        "abstain" => Some(1),
        "revise" => Some(2),
        "reject" => Some(3),
        _ => None,
    }
}

fn aggregate_artifact_verdict<'a>(
    verdicts: impl IntoIterator<Item = &'a str>,
) -> Option<&'static str> {
    let rank = verdicts
        .into_iter()
        .filter_map(artifact_verdict_rank)
        .max()?;
    Some(match rank {
        0 => "approve",
        1 => "abstain",
        2 => "revise",
        _ => "reject",
    })
}

fn artifact_reference_sets(artifact: &Value) -> BTreeMap<&'static str, BTreeSet<&str>> {
    [
        "candidates",
        "paths",
        "findings",
        "relationships",
        "clusters",
        "excerpts",
        "policies",
        "verification",
    ]
    .into_iter()
    .map(|category| {
        let values = artifact
            .pointer(&format!("/context/reference_index/{category}"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        (category, values)
    })
    .collect()
}

fn validate_artifact_citations(
    citations: &Value,
    references: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<()> {
    let mut count = 0_usize;
    for (category, available) in references {
        let supplied = citations
            .get(*category)
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("advice artifact citation set is incomplete"))?;
        count = count.saturating_add(supplied.len());
        for reference in supplied.iter().filter_map(Value::as_str) {
            if !available.contains(reference) {
                bail!(
                    "advice artifact contains an invented or unavailable {category} citation {reference:?}"
                );
            }
        }
    }
    if count == 0 {
        bail!("advice artifact rationale has no supplied evidence citation");
    }
    Ok(())
}

fn validate_artifact_semantics(artifact: &Value) -> Result<()> {
    let references = artifact_reference_sets(artifact);
    let expected_candidates = &references["candidates"];
    let candidate_ids = artifact
        .get("candidate_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if &candidate_ids != expected_candidates {
        bail!("advice artifact candidate identity drifted from its reference index");
    }
    let candidates = artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("advice artifact has no candidate evaluations"))?;
    let mut observed_candidates = BTreeSet::new();
    let mut candidate_verdicts = Vec::new();
    for candidate in candidates {
        let candidate_id = candidate
            .get("candidate_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("advice artifact candidate identity is missing"))?;
        if !expected_candidates.contains(candidate_id) || !observed_candidates.insert(candidate_id)
        {
            bail!("advice artifact contains an unknown or duplicate candidate {candidate_id}");
        }
        validate_artifact_citations(&candidate["citations"], &references)?;
        let rules = candidate["rule_evaluations"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("advice artifact rule evaluations are missing"))?;
        let mut observed_rules = BTreeSet::new();
        for rule in rules {
            let rule_id = rule["rule_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("advice artifact rule identity is missing"))?;
            if !references["policies"].contains(rule_id) || !observed_rules.insert(rule_id) {
                bail!("advice artifact contains an unknown or duplicate policy rule {rule_id}");
            }
            validate_artifact_citations(&rule["citations"], &references)?;
            if !rule["citations"]["policies"]
                .as_array()
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(rule_id)))
            {
                bail!("advice artifact rule {rule_id} does not cite its supplied policy ID");
            }
        }
        if observed_rules != references["policies"] {
            bail!("advice artifact candidate {candidate_id} has an incomplete policy matrix");
        }
        let computed =
            aggregate_artifact_verdict(rules.iter().filter_map(|rule| rule["verdict"].as_str()))
                .ok_or_else(|| anyhow::anyhow!("advice artifact has no policy verdicts"))?;
        if candidate["aggregate_verdict"].as_str() != Some(computed) {
            bail!("advice artifact candidate {candidate_id} has a stale aggregate verdict");
        }
        candidate_verdicts.push(computed);
    }
    if &observed_candidates != expected_candidates {
        bail!("advice artifact is missing one or more candidate evaluations");
    }
    let computed = aggregate_artifact_verdict(candidate_verdicts)
        .ok_or_else(|| anyhow::anyhow!("advice artifact has no aggregate verdict evidence"))?;
    if artifact
        .pointer("/evaluation/aggregate_verdict")
        .and_then(Value::as_str)
        != Some(computed)
    {
        bail!("advice artifact has a stale recomputed aggregate verdict");
    }
    if artifact.pointer("/evaluation/warnings") != artifact.pointer("/validation/warnings") {
        bail!("advice artifact validation warnings drifted from evaluation evidence");
    }
    Ok(())
}
