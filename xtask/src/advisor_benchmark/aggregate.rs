fn manual_gates_pass(thresholds: &Thresholds, manual: Option<&ManualScores>) -> bool {
    manual.is_some_and(|scores| {
        scores.recommendation_usefulness_mean >= thresholds.maintainer_usefulness_mean_minimum
            && scores.overall_quality_mean >= thresholds.manual_quality_mean_minimum
            && scores.unsupported_claim_count <= thresholds.unsupported_claim_count_maximum
    })
}

struct OutputInputs<'a> {
    corpus: &'a Corpus,
    reports: &'a BTreeMap<String, PreparedReport>,
    thresholds: &'a Thresholds,
}

fn write_outputs(
    options: &Options,
    inputs: &OutputInputs<'_>,
    started: u128,
    samples: &[Sample],
    manual: Option<&ManualScores>,
    termination_reason: Option<&str>,
) -> Result<(PathBuf, PathBuf)> {
    let OutputInputs {
        corpus,
        reports,
        thresholds,
    } = inputs;
    let report_digests = reports
        .iter()
        .map(|(key, report)| (key.clone(), report.sha256.clone()))
        .collect::<BTreeMap<_, _>>();
    verify_sample_matrix(
        options,
        corpus,
        &report_digests,
        samples,
        termination_reason.is_none(),
    )?;
    let corpus_pinned = corpus.repositories.iter().all(|(key, repository)| {
        repository.expected_report_sha256.as_deref()
            == reports.get(key).map(|report| report.sha256.as_str())
    });
    let successful = samples
        .iter()
        .filter(|sample| sample.status == "valid")
        .count();
    let matched_rules = samples
        .iter()
        .map(|sample| sample.matched_rule_verdicts)
        .sum::<usize>();
    let expected_rules = samples
        .iter()
        .map(|sample| sample.expected_rule_verdicts)
        .sum::<usize>();
    let structured_rate = successful as f64 / samples.len().max(1) as f64;
    let rule_accuracy = matched_rules as f64 / expected_rules.max(1) as f64;
    let aggregate_accuracy = samples
        .iter()
        .filter(|sample| sample.aggregate_match)
        .count() as f64
        / samples.len().max(1) as f64;
    let citation_completeness = samples
        .iter()
        .filter(|sample| sample.citation_complete)
        .count() as f64
        / samples.len().max(1) as f64;
    let consistency = verdict_consistency(samples);
    let abstention = samples
        .iter()
        .filter(|sample| sample.expected_aggregate == "abstain")
        .collect::<Vec<_>>();
    let abstention_recall = if abstention.is_empty() {
        0.0
    } else {
        abstention
            .iter()
            .filter(|sample| sample.reported_aggregate.as_deref() == Some("abstain"))
            .count() as f64
            / abstention.len() as f64
    };
    let top_one_p95 = p95(
        samples
            .iter()
            .filter(|sample| {
                sample.status == "valid" && sample.phase == "warm" && sample.candidate_count == 1
            })
            .map(|sample| sample.total_elapsed_ms as u64)
            .collect(),
    );
    let top_five_p95 = p95(
        samples
            .iter()
            .filter(|sample| {
                sample.status == "valid" && sample.phase == "warm" && sample.candidate_count == 5
            })
            .map(|sample| sample.total_elapsed_ms as u64)
            .collect(),
    );
    let top_three_p95 = p95(
        samples
            .iter()
            .filter(|sample| {
                sample.status == "valid" && sample.phase == "warm" && sample.candidate_count == 3
            })
            .map(|sample| sample.total_elapsed_ms as u64)
            .collect(),
    );
    let peak_rss = samples
        .iter()
        .filter_map(|sample| sample.peak_process_rss_bytes)
        .max();
    let swap_growth = samples
        .iter()
        .filter_map(|sample| sample.swap_growth_bytes)
        .max();
    let minimum_available_memory = samples
        .iter()
        .filter_map(|sample| sample.system_available_memory_minimum_bytes)
        .min();
    let retries = samples
        .iter()
        .map(|sample| sample.retry_count)
        .sum::<u64>();
    let invalid = samples
        .iter()
        .map(|sample| sample.accepted_invalid_references)
        .sum::<u64>();
    let truth_changes = samples
        .iter()
        .map(|sample| sample.accepted_detector_truth_changes)
        .sum::<u64>();
    let runtime_phase_metrics_complete = successful > 0
        && samples
            .iter()
            .filter(|sample| sample.status == "valid")
            .all(|sample| {
                sample.model_load_duration_ns.is_some()
                    && sample.prompt_eval_duration_ns.is_some()
                    && sample.generation_duration_ns.is_some()
                    && sample.input_tokens.is_some()
                    && sample.output_tokens.is_some()
            });
    let matrix_completed = termination_reason.is_none();
    let recommended = matrix_completed
        .then(|| recommended_configuration(options, thresholds, samples))
        .flatten();
    let automatic_gates = corpus_pinned
        && matrix_completed
        && release_matrix_complete(options)
        && recommended.is_some()
        && runtime_phase_metrics_complete
        && structured_rate >= thresholds.structured_output_success_rate_minimum
        && rule_accuracy >= thresholds.high_severity_rule_accuracy_minimum
        && aggregate_accuracy >= thresholds.aggregate_verdict_accuracy_minimum
        && citation_completeness >= thresholds.citation_completeness_minimum
        && consistency >= thresholds.repeated_verdict_consistency_minimum
        && invalid <= thresholds.accepted_invalid_reference_maximum
        && truth_changes <= thresholds.accepted_detector_truth_change_maximum
        && abstention_recall >= thresholds.abstention_recall_minimum
        && top_one_p95.is_some_and(|value| value <= thresholds.warm_top_one_p95_ms_maximum)
        && top_five_p95.is_some_and(|value| value <= thresholds.warm_top_five_p95_ms_maximum)
        && peak_rss.is_some_and(|value| value <= thresholds.peak_process_rss_bytes_maximum)
        && swap_growth.is_some_and(|value| value <= thresholds.swap_growth_bytes_maximum);
    let manual_gates = manual_gates_pass(thresholds, manual);
    let recommendation = if automatic_gates && manual_gates {
        "ship"
    } else if successful > 0 {
        "adjust"
    } else {
        "defer"
    };
    let summary = json!({
        "sample_count": samples.len(), "valid_sample_count": successful,
        "structured_output_success_rate": structured_rate,
        "high_severity_rule_accuracy": rule_accuracy,
        "aggregate_verdict_accuracy": aggregate_accuracy,
        "citation_completeness": citation_completeness,
        "repeated_verdict_consistency": consistency,
        "accepted_invalid_references": invalid,
        "accepted_detector_truth_changes": truth_changes,
        "runtime_phase_metrics_complete": runtime_phase_metrics_complete,
        "abstention_recall": abstention_recall,
        "manual_quality": manual,
        "warm_top_one_p95_ms": top_one_p95, "warm_top_three_p95_ms": top_three_p95, "warm_top_five_p95_ms": top_five_p95,
        "peak_process_rss_bytes": peak_rss, "minimum_system_available_memory_bytes": minimum_available_memory,
        "maximum_swap_growth_bytes": swap_growth, "retry_count": retries,
        "automatic_gates_passed": automatic_gates, "manual_quality_gates_passed": manual_gates,
        "corpus_report_fingerprints_pinned": corpus_pinned,
        "matrix_completed": matrix_completed,
        "termination_reason": termination_reason
    });
    let repositories = corpus
        .repositories
        .iter()
        .map(|(key, fixture)| {
            let report_sha256 = reports.get(key).map(|report| report.sha256.as_str());
            (
                key,
                json!({
                    "revision": fixture.revision,
                    "as_of": fixture.as_of,
                    "report_sha256": report_sha256,
                    "matches_expected": fixture.expected_report_sha256.as_deref() == report_sha256
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut result = json!({
        "schema_version": 1,
        "status": if matrix_completed { "complete" } else { "incomplete" },
        "started_unix_ms": started,
        "finished_unix_ms": now_ms(),
        "configuration": {
            "corpus": "corpus-v1", "thresholds": "thresholds-v1",
            "corpus_sha256": sha256(&fs::read(resolve(&options.repo_root, &options.corpus))?),
            "thresholds_sha256": sha256(&fs::read(resolve(&options.repo_root, &options.thresholds))?),
            "provider": options.provider, "runtime_label": options.runtime_label, "runtime_model": options.runtime_model,
            "model": options.model, "model_digest": options.model_digest,
            "model_quantization": options.model_quantization,
            "model_size_bytes": options.model_size_bytes,
            "estimated_peak_memory_bytes": options.estimated_peak_memory_bytes,
            "dedicated_host_confirmed": options.confirm_dedicated_host,
            "initial_runtime_state": options.initial_runtime_state,
            "runtime_context_tokens": BENCHMARK_RUNTIME_CONTEXT_TOKENS,
            "request_timeout_seconds": BENCHMARK_TIMEOUT_SECONDS,
            "child_output_limit_bytes": BENCHMARK_CHILD_OUTPUT_LIMIT_BYTES,
            "endpoint_classification": "loopback",
            "repetitions": options.repetitions, "full_matrix": options.full_matrix,
            "repository_keys": reports.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            "repository_revisions": corpus.repositories.iter().map(|(key, fixture)| (key, fixture.revision.as_str())).collect::<BTreeMap<_, _>>()
        },
        "system": system_profile(),
        "thresholds": thresholds,
        "summary": summary,
        "samples": samples,
        "repositories": repositories,
        "recommended_configuration": recommended.clone(),
        "recommendation": recommendation
    });
    if !matrix_completed {
        let next_step = if termination_reason.is_some_and(|reason| {
            matches!(
                reason,
                "provider_model_identity_missing" | "provider_model_mismatch"
            )
        }) {
            "Do not retry or accept evidence from this runtime. Verify the separately provisioned served-model identity before authorizing a fresh run."
        } else if termination_reason == Some("benchmark_child_output_limit") {
            "Do not retry until the unexpected child output volume is understood and bounded. Inspect the retained diagnostics without starting a provider on this host."
        } else {
            "Do not retry on this host. Inspect the safety-guard result, recover the runtime separately, and use a different adequately resourced dedicated host."
        };
        result.as_object_mut().expect("benchmark result is an object").insert(
            "next_step".to_string(),
            json!(next_step),
        );
    }
    let output_dir = resolve(&options.repo_root, &options.output_dir);
    let json_path = output_dir.join("results.json");
    let markdown_path = output_dir.join("decision.md");
    let markdown = render_live_decision(&result)?;
    write_benchmark_pair(&json_path, &result, &markdown_path, &markdown)?;
    Ok((json_path, markdown_path))
}
