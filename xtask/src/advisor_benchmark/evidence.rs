fn expected_contexts(full_matrix: bool, candidate_count: usize) -> &'static [usize] {
    if !full_matrix {
        return &[8_192];
    }
    match candidate_count {
        1 => &[2_048, 4_096, 8_192],
        2 | 3 => &[4_096, 8_192],
        _ => &[8_192],
    }
}

fn expected_efforts(full_matrix: bool) -> &'static [&'static str] {
    if full_matrix {
        &["low", "medium", "high"]
    } else {
        &["medium"]
    }
}

fn verify_sample(
    sample: &Sample,
    case: &Case,
    report_sha256: &str,
    reasoning_effort: &str,
    context_token_limit: usize,
    repetition: usize,
    phase: &str,
) -> Result<()> {
    verify_sample_digest(sample)?;
    let expected_rule_verdicts =
        high_severity_expectation_count(&case.expected_rule_verdicts) * case.candidate_count;
    let expected_output_tokens = case.candidate_count.saturating_mul(2_048).min(8_192);
    let identity_matches = sample.case_id == case.id
        && sample.repository == case.repository
        && sample.scenario_tags == case.scenario_tags
        && sample.scenario == case.scenario
        && sample.candidate_count == case.candidate_count
        && sample.report_sha256 == report_sha256
        && sample.reasoning_effort == reasoning_effort
        && sample.context_token_limit == context_token_limit
        && sample.output_token_limit == expected_output_tokens
        && sample.repetition == repetition
        && sample.phase == phase
        && sample.expected_aggregate == case.expected_aggregate
        && sample.expected_rule_verdicts == expected_rule_verdicts;
    if !identity_matches {
        bail!(
            "benchmark sample {} does not match its pinned corpus matrix cell",
            sample.case_id
        );
    }
    if sample.matched_rule_verdicts > sample.expected_rule_verdicts {
        bail!(
            "benchmark sample {} reports more matched rules than expected rules",
            sample.case_id
        );
    }
    let aggregate_match = sample.reported_aggregate.as_deref() == Some(&case.expected_aggregate);
    if sample.aggregate_match != aggregate_match {
        bail!(
            "benchmark sample {} has inconsistent aggregate_match evidence",
            sample.case_id
        );
    }
    let candidate_count_matches = sample.actual_candidate_count == Some(case.candidate_count);
    if sample.status == "valid" && !candidate_count_matches {
        bail!(
            "benchmark sample {} has inconsistent candidate-count status",
            sample.case_id
        );
    }
    if sample.status == "valid"
        && (sample.artifact_sha256.is_none()
            || sample.exit_code != Some(0)
            || sample.failure_category.is_some()
            || sample.accepted_invalid_references != 0)
    {
        bail!(
            "benchmark sample {} has inconsistent successful process evidence",
            sample.case_id
        );
    }
    if sample.status != "valid" && sample.failure_category.is_none() {
        bail!(
            "benchmark sample {} is failed without a failure category",
            sample.case_id
        );
    }
    Ok(())
}

fn verify_sample_matrix(
    options: &Options,
    corpus: &Corpus,
    report_digests: &BTreeMap<String, String>,
    samples: &[Sample],
    require_complete: bool,
) -> Result<()> {
    let efforts = expected_efforts(options.full_matrix);
    let mut index = 0usize;
    for case in &corpus.cases {
        let report_sha256 = report_digests.get(&case.repository).ok_or_else(|| {
            anyhow::anyhow!(
                "benchmark corpus case {} has no prepared repository fingerprint",
                case.id
            )
        })?;
        for effort in efforts {
            for context in expected_contexts(options.full_matrix, case.candidate_count) {
                for repetition in 1..=options.repetitions {
                    let Some(sample) = samples.get(index) else {
                        if require_complete {
                            bail!("completed benchmark sample matrix ended before cell {index}");
                        }
                        return Ok(());
                    };
                    let phase = if index == 0 && options.initial_runtime_state == "cold" {
                        "cold"
                    } else {
                        "warm"
                    };
                    verify_sample(
                        sample,
                        case,
                        report_sha256,
                        effort,
                        *context,
                        repetition,
                        phase,
                    )?;
                    index += 1;
                }
            }
        }
    }
    if samples.len() != index {
        bail!("benchmark sample matrix contains unexpected extra cells");
    }
    Ok(())
}

fn verify_complete_result_bindings(
    result: &Value,
    corpus: &Corpus,
    options: &Options,
    samples: &[Sample],
) -> Result<()> {
    let started = result["started_unix_ms"].as_u64().expect("validated start time");
    let finished = result["finished_unix_ms"]
        .as_u64()
        .expect("validated finish time");
    if finished < started {
        bail!("completed benchmark finish time precedes its start time");
    }

    let configuration = &result["configuration"];
    let expected_keys = corpus.repositories.keys().cloned().collect::<BTreeSet<_>>();
    let actual_keys = configuration["repository_keys"]
        .as_array()
        .expect("validated repository keys")
        .iter()
        .map(|value| value.as_str().expect("validated repository key").to_string())
        .collect::<BTreeSet<_>>();
    if actual_keys != expected_keys {
        bail!("completed benchmark repository keys do not match the pinned corpus");
    }
    let expected_revisions = corpus
        .repositories
        .iter()
        .map(|(key, fixture)| (key.clone(), fixture.revision.clone()))
        .collect::<BTreeMap<_, _>>();
    if configuration.get("repository_revisions")
        != Some(&serde_json::to_value(&expected_revisions)?)
    {
        bail!("completed benchmark repository revisions do not match the pinned corpus");
    }

    let repositories = result["repositories"]
        .as_object()
        .expect("validated repositories");
    if repositories.keys().cloned().collect::<BTreeSet<_>>() != expected_keys {
        bail!("completed benchmark repository evidence does not match the pinned corpus");
    }
    let mut report_digests = BTreeMap::new();
    for (key, fixture) in &corpus.repositories {
        let repository = &repositories[key];
        let report_sha256 = repository["report_sha256"]
            .as_str()
            .expect("validated report digest");
        let expected_match = fixture.expected_report_sha256.as_deref() == Some(report_sha256);
        if repository["revision"] != fixture.revision
            || repository["as_of"] != fixture.as_of
            || repository["matches_expected"] != expected_match
        {
            bail!(
                "completed benchmark repository {key} does not match its pinned corpus evidence"
            );
        }
        report_digests.insert(key.clone(), report_sha256.to_string());
    }
    verify_sample_matrix(options, corpus, &report_digests, samples, true)
}
