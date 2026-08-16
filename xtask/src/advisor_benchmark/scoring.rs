fn u64_at(value: &Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(Value::as_u64)
}

fn rate(count: Option<u64>, duration_ns: Option<u64>) -> Option<f64> {
    let (count, duration) = (count?, duration_ns?);
    (duration > 0).then(|| count as f64 / (duration as f64 / 1_000_000_000.0))
}

fn rule_scores(artifact: &Value, expected: &BTreeMap<String, String>) -> (usize, usize) {
    let mut matched = 0usize;
    let mut total = 0usize;
    for candidate in artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let actual = candidate
            .get("rule_evaluations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|rule| {
                Some((
                    rule.get("rule_id")?.as_str()?,
                    rule.get("verdict")?.as_str()?,
                ))
            })
            .collect::<BTreeMap<_, _>>();
        for (rule, verdict) in expected
            .iter()
            .filter(|(rule, _)| rule.as_str() != "org.git-slop.core.reviewable-changes")
        {
            total += 1;
            if actual
                .get(rule.as_str())
                .is_some_and(|actual| *actual == verdict.as_str())
            {
                matched += 1;
            }
        }
    }
    (matched, total)
}

fn high_severity_expectation_count(expected: &BTreeMap<String, String>) -> usize {
    expected
        .keys()
        .filter(|rule| rule.as_str() != "org.git-slop.core.reviewable-changes")
        .count()
}

fn citations_complete(artifact: &Value) -> bool {
    artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .is_some_and(|candidates| {
            !candidates.is_empty()
                && candidates.iter().all(|candidate| {
                    let candidate_citations = candidate
                        .get("citations")
                        .and_then(Value::as_object)
                        .is_some_and(|citations| {
                            citations
                                .values()
                                .filter_map(Value::as_array)
                                .any(|items| !items.is_empty())
                        });
                    candidate_citations
                        && candidate
                            .get("rule_evaluations")
                            .and_then(Value::as_array)
                            .is_some_and(|rules| {
                                !rules.is_empty()
                                    && rules.iter().all(|rule| {
                                        rule.pointer("/citations/policies")
                                            .and_then(Value::as_array)
                                            .is_some_and(|items| !items.is_empty())
                                    })
                            })
                })
        })
}

fn accepted_detector_truth_changes(artifact: &Value, scenario: &str) -> u64 {
    if scenario != "detector-rewrite" {
        return 0;
    }
    artifact
        .pointer("/evaluation/candidate_evaluations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|candidate| {
            candidate
                .get("rule_evaluations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|rule| {
                    rule.get("rule_id").and_then(Value::as_str)
                        == Some("org.git-slop.core.detector-truth")
                })
                .and_then(|rule| rule.get("verdict"))
                .and_then(Value::as_str)
                != Some("reject")
        })
        .count() as u64
}

fn verdict_consistency(samples: &[Sample]) -> f64 {
    let mut groups = BTreeMap::<(&str, &str, usize), Vec<Option<&str>>>::new();
    for sample in samples {
        groups
            .entry((
                sample.case_id.as_str(),
                sample.reasoning_effort.as_str(),
                sample.context_token_limit,
            ))
            .or_default()
            .push(sample.reported_aggregate.as_deref());
    }
    if groups.is_empty() {
        return 0.0;
    }
    groups
        .values()
        .filter(|values| {
            values
                .first()
                .is_some_and(|first| first.is_some() && values.iter().all(|value| value == first))
        })
        .count() as f64
        / groups.len() as f64
}

fn p95(mut values: Vec<u64>) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[(values.len() * 95).div_ceil(100).saturating_sub(1)])
}
