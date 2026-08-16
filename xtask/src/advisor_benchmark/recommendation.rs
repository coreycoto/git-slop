fn recommended_configuration(
    options: &Options,
    thresholds: &Thresholds,
    samples: &[Sample],
) -> Option<Value> {
    if !release_matrix_complete(options) {
        return None;
    }
    let expected_cases = samples
        .iter()
        .map(|sample| sample.case_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut best: Option<(f64, u64, Value)> = None;
    for effort in ["low", "medium", "high"] {
        let selected = samples
            .iter()
            .filter(|sample| {
                sample.reasoning_effort == effort && sample.context_token_limit == 8_192
            })
            .collect::<Vec<_>>();
        let actual_cases = selected
            .iter()
            .map(|sample| sample.case_id.as_str())
            .collect::<BTreeSet<_>>();
        if selected.is_empty() || actual_cases != expected_cases {
            continue;
        }
        let valid = selected
            .iter()
            .filter(|sample| sample.status == "valid")
            .count();
        let matched = selected
            .iter()
            .map(|sample| sample.matched_rule_verdicts)
            .sum::<usize>();
        let expected = selected
            .iter()
            .map(|sample| sample.expected_rule_verdicts)
            .sum::<usize>();
        let structured = valid as f64 / selected.len() as f64;
        let rule_accuracy = matched as f64 / expected.max(1) as f64;
        let aggregate_accuracy = selected
            .iter()
            .filter(|sample| sample.aggregate_match)
            .count() as f64
            / selected.len() as f64;
        let citations = selected
            .iter()
            .filter(|sample| sample.citation_complete)
            .count() as f64
            / selected.len() as f64;
        let consistency = verdict_consistency(
            &selected
                .iter()
                .map(|sample| (*sample).clone())
                .collect::<Vec<_>>(),
        );
        let abstention = selected
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
        let invalid = selected
            .iter()
            .map(|sample| sample.accepted_invalid_references)
            .sum::<u64>();
        let phase_metrics_complete = selected.iter().all(|sample| {
            sample.model_load_duration_ns.is_some()
                && sample.prompt_eval_duration_ns.is_some()
                && sample.generation_duration_ns.is_some()
                && sample.input_tokens.is_some()
                && sample.output_tokens.is_some()
        });
        let truth_changes = selected
            .iter()
            .map(|sample| sample.accepted_detector_truth_changes)
            .sum::<u64>();
        let top_one_p95 = p95(
            selected
                .iter()
                .filter(|sample| sample.phase == "warm" && sample.candidate_count == 1)
                .map(|sample| sample.total_elapsed_ms as u64)
                .collect(),
        );
        let top_five_p95 = p95(
            selected
                .iter()
                .filter(|sample| sample.phase == "warm" && sample.candidate_count == 5)
                .map(|sample| sample.total_elapsed_ms as u64)
                .collect(),
        );
        let peak_rss = selected
            .iter()
            .filter_map(|sample| sample.peak_process_rss_bytes)
            .max();
        let swap_growth = selected
            .iter()
            .filter_map(|sample| sample.swap_growth_bytes)
            .max();
        let passes = phase_metrics_complete
            && structured >= thresholds.structured_output_success_rate_minimum
            && rule_accuracy >= thresholds.high_severity_rule_accuracy_minimum
            && aggregate_accuracy >= thresholds.aggregate_verdict_accuracy_minimum
            && citations >= thresholds.citation_completeness_minimum
            && consistency >= thresholds.repeated_verdict_consistency_minimum
            && abstention_recall >= thresholds.abstention_recall_minimum
            && invalid <= thresholds.accepted_invalid_reference_maximum
            && truth_changes <= thresholds.accepted_detector_truth_change_maximum
            && top_one_p95
                .is_some_and(|value| value <= thresholds.warm_top_one_p95_ms_maximum)
            && top_five_p95
                .is_some_and(|value| value <= thresholds.warm_top_five_p95_ms_maximum)
            && peak_rss
                .is_some_and(|value| value <= thresholds.peak_process_rss_bytes_maximum)
            && swap_growth.is_some_and(|value| value <= thresholds.swap_growth_bytes_maximum);
        if !passes {
            continue;
        }
        let quality = rule_accuracy + aggregate_accuracy + citations + consistency + abstention_recall;
        let latency = top_five_p95.unwrap_or(u64::MAX);
        let value = json!({
            "provider": options.provider,
            "model": options.model,
            "runtime_label": options.runtime_label,
            "runtime_model": options.runtime_model,
            "model_digest": options.model_digest,
            "model_quantization": options.model_quantization,
            "model_size_bytes": options.model_size_bytes,
            "estimated_peak_memory_bytes": options.estimated_peak_memory_bytes,
            "reasoning_effort": effort,
            "max_context_tokens": 8_192,
            "capacity_strategy": {"top_one_minimum_tokens": 2_048, "top_three_minimum_tokens": 4_096, "top_five_minimum_tokens": 8_192},
            "selection_metrics": {
                "structured_output_success_rate": structured,
                "high_severity_rule_accuracy": rule_accuracy,
                "aggregate_verdict_accuracy": aggregate_accuracy,
                "citation_completeness": citations,
                "repeated_verdict_consistency": consistency,
                "abstention_recall": abstention_recall,
                "warm_top_one_p95_ms": top_one_p95,
                "warm_top_five_p95_ms": top_five_p95,
                "peak_process_rss_bytes": peak_rss,
                "maximum_swap_growth_bytes": swap_growth
            }
        });
        let replace = best.as_ref().is_none_or(|(best_quality, best_latency, _)| {
            quality > *best_quality + f64::EPSILON
                || ((quality - *best_quality).abs() <= f64::EPSILON
                    && latency < *best_latency)
        });
        if replace {
            best = Some((quality, latency, value));
        }
    }
    best.map(|(_, _, value)| value)
}
