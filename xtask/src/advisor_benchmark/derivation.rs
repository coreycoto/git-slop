use super::*;

pub(super) struct BenchmarkDerivation {
    pub(super) status: BenchmarkStatus,
    pub(super) termination: Option<BenchmarkTermination>,
    pub(super) summary: Value,
    pub(super) recommended_configuration: Option<Value>,
    pub(super) recommendation: Recommendation,
}

fn manual_gates_pass(thresholds: &Thresholds, manual: Option<&ManualScores>) -> bool {
    manual.is_some_and(|scores| {
        scores.recommendation_usefulness_mean >= thresholds.maintainer_usefulness_mean_minimum
            && scores.overall_quality_mean >= thresholds.manual_quality_mean_minimum
            && scores.unsupported_claim_count <= thresholds.unsupported_claim_count_maximum
    })
}

pub(super) fn derive_benchmark(
    options: &Options,
    thresholds: &Thresholds,
    samples: &[Sample],
    corpus_pinned: bool,
    manual: Option<&ManualScores>,
    termination_reason: Option<&str>,
) -> Result<BenchmarkDerivation> {
    let termination = termination_reason
        .map(BenchmarkTermination::parse)
        .transpose()?;
    let status = if termination.is_some() {
        BenchmarkStatus::Incomplete
    } else {
        BenchmarkStatus::Complete
    };
    let valid_sample_count = samples
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
    let sample_count = samples.len().max(1) as f64;
    let structured_rate = valid_sample_count as f64 / sample_count;
    let rule_accuracy = matched_rules as f64 / expected_rules.max(1) as f64;
    let aggregate_accuracy = samples
        .iter()
        .filter(|sample| sample.aggregate_match)
        .count() as f64
        / sample_count;
    let citation_completeness = samples
        .iter()
        .filter(|sample| sample.citation_complete)
        .count() as f64
        / sample_count;
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
    let warm_p95 = |candidate_count| {
        p95(samples
            .iter()
            .filter(|sample| {
                sample.status == "valid"
                    && sample.phase == "warm"
                    && sample.candidate_count == candidate_count
            })
            .map(|sample| sample.total_elapsed_ms as u64)
            .collect())
    };
    let top_one_p95 = warm_p95(1);
    let top_three_p95 = warm_p95(3);
    let top_five_p95 = warm_p95(5);
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
    let retries = samples.iter().map(|sample| sample.retry_count).sum::<u64>();
    let invalid = samples
        .iter()
        .map(|sample| sample.accepted_invalid_references)
        .sum::<u64>();
    let truth_changes = samples
        .iter()
        .map(|sample| sample.accepted_detector_truth_changes)
        .sum::<u64>();
    let runtime_phase_metrics_complete = valid_sample_count > 0
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
    let matrix_completed = status == BenchmarkStatus::Complete;
    let recommended_configuration = matrix_completed
        .then(|| recommended_configuration(options, thresholds, samples))
        .flatten();
    let automatic_gates_passed = corpus_pinned
        && matrix_completed
        && release_matrix_complete(options)
        && recommended_configuration.is_some()
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
    let manual_quality_gates_passed = manual_gates_pass(thresholds, manual);
    let recommendation = if automatic_gates_passed && manual_quality_gates_passed {
        Recommendation::Ship
    } else if valid_sample_count > 0 {
        Recommendation::Adjust
    } else {
        Recommendation::Defer
    };
    let summary = json!({
        "sample_count": samples.len(), "valid_sample_count": valid_sample_count,
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
        "warm_top_one_p95_ms": top_one_p95,
        "warm_top_three_p95_ms": top_three_p95,
        "warm_top_five_p95_ms": top_five_p95,
        "peak_process_rss_bytes": peak_rss,
        "minimum_system_available_memory_bytes": minimum_available_memory,
        "maximum_swap_growth_bytes": swap_growth,
        "retry_count": retries,
        "automatic_gates_passed": automatic_gates_passed,
        "manual_quality_gates_passed": manual_quality_gates_passed,
        "corpus_report_fingerprints_pinned": corpus_pinned,
        "matrix_completed": matrix_completed,
        "termination_reason": termination
    });
    Ok(BenchmarkDerivation {
        status,
        termination,
        summary,
        recommended_configuration,
        recommendation,
    })
}
