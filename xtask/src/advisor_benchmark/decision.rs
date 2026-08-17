fn required_u64(value: &Value, pointer: &str) -> Result<u64> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("benchmark decision field is missing: {pointer}"))
}

fn required_f64(value: &Value, pointer: &str) -> Result<f64> {
    value
        .pointer(pointer)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow::anyhow!("benchmark decision field is missing: {pointer}"))
}

fn nullable_count(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .map_or_else(|| "unavailable".to_string(), |count| count.to_string())
}

fn render_live_decision(result: &Value) -> Result<String> {
    let summary = result
        .get("summary")
        .ok_or_else(|| anyhow::anyhow!("benchmark result has no summary"))?;
    let recommendation = result["recommendation"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("benchmark result has no recommendation"))?;
    let matrix_completed = summary["matrix_completed"]
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("benchmark result has no matrix completion state"))?;
    let termination_reason = summary["termination_reason"].as_str().unwrap_or("none");
    let recommended_text = result["recommended_configuration"].as_object().map_or_else(
        || {
            "unavailable; no default configuration passed every automatic quality and performance gate"
                .to_string()
        },
        |configuration| {
            format!(
                "{} via {} with {} reasoning and an {}-token default context",
                configuration["runtime_label"]
                    .as_str()
                    .unwrap_or("unknown runtime"),
                configuration["provider"]
                    .as_str()
                    .unwrap_or("unknown provider"),
                configuration["reasoning_effort"]
                    .as_str()
                    .unwrap_or("unknown"),
                configuration["max_context_tokens"]
                    .as_u64()
                    .unwrap_or_default()
            )
        },
    );
    let manual = summary["manual_quality"].as_object();
    let manual_value = |key: &str| {
        manual
            .and_then(|scores| scores.get(key))
            .and_then(Value::as_f64)
            .map_or_else(|| "not reviewed".to_string(), |score| format!("{score:.2}"))
    };
    let unsupported = manual
        .and_then(|scores| scores.get("unsupported_claim_count"))
        .and_then(Value::as_u64)
        .map_or_else(|| "not reviewed".to_string(), |count| count.to_string());
    let ratings_digest = result
        .get("manual_ratings_sha256")
        .and_then(Value::as_str)
        .map_or_else(String::new, |digest| {
            format!("- Manual ratings SHA-256: {digest}\n")
        });

    Ok(format!(
        "# Safeguard-only V1 decision\n\n- Recommendation: **{recommendation}**\n- Matrix completed: **{matrix_completed}**\n- Termination reason: {termination_reason}\n- Recommended configuration: {recommended_text}\n- Model quantization: {}\n- Corpus report fingerprints pinned: **{}**\n- Runtime phase metrics complete: **{}**\n- Valid structured outputs: {}/{} ({:.3})\n- High-severity rule accuracy: {:.3}\n- Aggregate verdict accuracy: {:.3}\n- Citation completeness: {:.3}\n- Repeated-verdict consistency: {:.3}\n- Abstention recall: {:.3}\n- Warm top-one p95: {} ms\n- Warm top-three p95: {} ms\n- Warm top-five p95: {} ms\n- Peak process RSS: {} bytes\n- Minimum observed system-available memory: {} bytes\n- Maximum observed swap growth: {} bytes\n- Retries: {}\n- Maintainer usefulness mean: {}\n- Manual quality mean: {}\n- Unsupported claims found by maintainers: {}\n{ratings_digest}\nNo raw repository content, prompts, paths, rationales, or private skill content is included in this report. Inspect ephemeral advice output during the run for case-level review; it is removed automatically.\n",
        result["configuration"]["model_quantization"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("benchmark result has no model quantization"))?,
        summary["corpus_report_fingerprints_pinned"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("benchmark result has no corpus pin state"))?,
        summary["runtime_phase_metrics_complete"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("benchmark result has no runtime metric state"))?,
        required_u64(result, "/summary/valid_sample_count")?,
        required_u64(result, "/summary/sample_count")?,
        required_f64(result, "/summary/structured_output_success_rate")?,
        required_f64(result, "/summary/high_severity_rule_accuracy")?,
        required_f64(result, "/summary/aggregate_verdict_accuracy")?,
        required_f64(result, "/summary/citation_completeness")?,
        required_f64(result, "/summary/repeated_verdict_consistency")?,
        required_f64(result, "/summary/abstention_recall")?,
        nullable_count(result, "/summary/warm_top_one_p95_ms"),
        nullable_count(result, "/summary/warm_top_three_p95_ms"),
        nullable_count(result, "/summary/warm_top_five_p95_ms"),
        nullable_count(result, "/summary/peak_process_rss_bytes"),
        nullable_count(result, "/summary/minimum_system_available_memory_bytes"),
        nullable_count(result, "/summary/maximum_swap_growth_bytes"),
        required_u64(result, "/summary/retry_count")?,
        manual_value("recommendation_usefulness_mean"),
        manual_value("overall_quality_mean"),
        unsupported,
    ))
}
