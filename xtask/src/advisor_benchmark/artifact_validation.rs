struct ArtifactAssessment {
    valid: bool,
    actual_candidate_count: Option<usize>,
    invalid_references: u64,
}

fn verdict_rank(value: &str) -> Option<u8> {
    match value {
        "approve" => Some(0),
        "abstain" => Some(1),
        "revise" => Some(2),
        "reject" => Some(3),
        _ => None,
    }
}

fn aggregate_verdict_strings<'a>(values: impl Iterator<Item = &'a str>) -> Option<&'static str> {
    let rank = values.filter_map(verdict_rank).max()?;
    Some(match rank {
        0 => "approve",
        1 => "abstain",
        2 => "revise",
        _ => "reject",
    })
}

fn citation_reference_errors(artifact: &Value) -> u64 {
    let categories = [
        "candidates",
        "paths",
        "findings",
        "relationships",
        "clusters",
        "excerpts",
        "policies",
        "verification",
    ];
    let references = categories
        .iter()
        .map(|category| {
            let values = artifact
                .pointer(&format!("/context/reference_index/{category}"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>();
            (*category, values)
        })
        .collect::<BTreeMap<_, _>>();
    let mut invalid = 0_u64;
    let mut inspect = |citations: Option<&Value>| {
        let Some(citations) = citations.and_then(Value::as_object) else {
            invalid = invalid.saturating_add(1);
            return;
        };
        for category in categories {
            let Some(values) = citations.get(category).and_then(Value::as_array) else {
                invalid = invalid.saturating_add(1);
                continue;
            };
            for value in values.iter().filter_map(Value::as_str) {
                if !references[category].contains(value) {
                    invalid = invalid.saturating_add(1);
                }
            }
        }
    };
    for candidate in artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        inspect(candidate.get("citations"));
        for rule in candidate
            .get("rule_evaluations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            inspect(rule.get("citations"));
        }
    }
    invalid
}

fn assess_advice_artifact(
    artifact: &Value,
    report: &PreparedReport,
    expected_candidate_count: usize,
) -> Result<ArtifactAssessment> {
    let schema: Value =
        serde_json::from_str(include_str!("../../../schemas/advice-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .context("embedded advice artifact schema is invalid")?;
    let schema_valid = validator.is_valid(artifact);
    let candidates = artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array);
    let actual_candidate_count = candidates.map(Vec::len);
    let recorded_ids = artifact
        .get("candidate_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let evaluated_ids = candidates
        .into_iter()
        .flatten()
        .filter_map(|candidate| candidate.get("candidate_id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let indexed_ids = artifact
        .pointer("/context/reference_index/candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let identity_valid = actual_candidate_count == Some(expected_candidate_count)
        && recorded_ids.len() == expected_candidate_count
        && recorded_ids == evaluated_ids
        && recorded_ids == indexed_ids;
    let raw_digest_valid = artifact
        .pointer("/report/sha256")
        .and_then(Value::as_str)
        == Some(report.raw_sha256.as_str());
    let canonical_digest_valid = artifact
        .pointer("/report/canonical_sha256")
        .and_then(Value::as_str)
        == Some(report.canonical_sha256.as_str());
    let mut aggregates_valid = true;
    let mut candidate_aggregates = Vec::new();
    for candidate in candidates.into_iter().flatten() {
        let computed = aggregate_verdict_strings(
            candidate
                .get("rule_evaluations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|rule| rule.get("verdict").and_then(Value::as_str)),
        );
        let recorded = candidate.get("aggregate_verdict").and_then(Value::as_str);
        aggregates_valid &= computed.is_some() && computed == recorded;
        if let Some(value) = computed {
            candidate_aggregates.push(value);
        }
    }
    let aggregate = aggregate_verdict_strings(candidate_aggregates.iter().copied());
    aggregates_valid &= aggregate
        == artifact
            .pointer("/evaluation/aggregate_verdict")
            .and_then(Value::as_str);
    let invalid_references = citation_reference_errors(artifact);
    let validation_claims = artifact.pointer("/validation/status").and_then(Value::as_str)
        == Some("valid")
        && artifact
            .pointer("/validation/aggregate_recomputed")
            .and_then(Value::as_bool)
            == Some(true)
        && artifact
            .pointer("/validation/references_validated")
            .and_then(Value::as_bool)
            == Some(true);
    Ok(ArtifactAssessment {
        valid: schema_valid
            && identity_valid
            && raw_digest_valid
            && canonical_digest_valid
            && aggregates_valid
            && validation_claims
            && invalid_references == 0,
        actual_candidate_count,
        invalid_references,
    })
}
