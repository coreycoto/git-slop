fn ratings(path: Option<&Path>, corpus: &Corpus) -> Result<Option<ManualScores>> {
    let Some(path) = path else { return Ok(None) };
    let bytes = read_bounded(path, MAX_BENCHMARK_CONFIG_BYTES, "advisor ratings")?;
    let ratings: RatingsFile = serde_json::from_slice(&bytes)?;
    let expected = corpus
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let actual = ratings
        .cases
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if ratings.schema_version != 1
        || ratings.reviewer_count == 0
        || ratings.reviewer_count > 20
        || expected != actual
    {
        bail!("maintainer ratings must cover every corpus case exactly");
    }
    let dimensions = ratings.cases.values().flat_map(|rating| {
        [
            rating.recommendation_usefulness,
            rating.fact_interpretation_separation,
            rating.scope_quality,
            rating.verification_quality,
            rating.actionability,
        ]
    });
    if dimensions
        .clone()
        .any(|rating| !(1.0..=5.0).contains(&rating))
    {
        bail!("every maintainer quality rating must be from 1 through 5");
    }
    let count = ratings.cases.len() as f64;
    let mean =
        |select: fn(&CaseRating) -> f64| ratings.cases.values().map(select).sum::<f64>() / count;
    let usefulness = mean(|rating| rating.recommendation_usefulness);
    let separation = mean(|rating| rating.fact_interpretation_separation);
    let scope = mean(|rating| rating.scope_quality);
    let verification = mean(|rating| rating.verification_quality);
    let actionability = mean(|rating| rating.actionability);
    Ok(Some(ManualScores {
        reviewer_count: ratings.reviewer_count,
        recommendation_usefulness_mean: usefulness,
        fact_interpretation_separation_mean: separation,
        scope_quality_mean: scope,
        verification_quality_mean: verification,
        actionability_mean: actionability,
        overall_quality_mean: (usefulness + separation + scope + verification + actionability)
            / 5.0,
        unsupported_claim_count: ratings
            .cases
            .values()
            .map(|rating| rating.unsupported_claim_count)
            .sum(),
    }))
}

fn verify_finalization_evidence(
    result: &Value,
    corpus: &Corpus,
    thresholds: &Thresholds,
) -> Result<bool> {
    let samples: Vec<Sample> = serde_json::from_value(
        result
            .get("samples")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("completed benchmark result is missing samples"))?,
    )
    .context("completed benchmark samples are invalid")?;
    let configuration = result
        .get("configuration")
        .ok_or_else(|| anyhow::anyhow!("completed benchmark result is missing configuration"))?;
    let options = Options {
        repo_root: PathBuf::new(),
        binary: PathBuf::new(),
        corpus: PathBuf::new(),
        thresholds: PathBuf::new(),
        repositories: Vec::new(),
        provider: configuration["provider"]
            .as_str()
            .expect("validated provider")
            .to_string(),
        endpoint: "loopback-redacted".to_string(),
        model: configuration["model"]
            .as_str()
            .expect("validated model")
            .to_string(),
        runtime_model: configuration["runtime_model"]
            .as_str()
            .expect("validated runtime model")
            .to_string(),
        runtime_label: configuration["runtime_label"]
            .as_str()
            .expect("validated runtime label")
            .to_string(),
        model_digest: configuration["model_digest"]
            .as_str()
            .expect("validated model digest")
            .to_string(),
        model_quantization: configuration["model_quantization"]
            .as_str()
            .expect("validated model quantization")
            .to_string(),
        model_size_bytes: configuration["model_size_bytes"].as_u64(),
        estimated_peak_memory_bytes: configuration["estimated_peak_memory_bytes"].as_u64(),
        confirm_dedicated_host: true,
        initial_runtime_state: configuration["initial_runtime_state"]
            .as_str()
            .expect("validated initial runtime state")
            .to_string(),
        output_dir: PathBuf::new(),
        repetitions: configuration["repetitions"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .expect("validated repetitions"),
        full_matrix: configuration["full_matrix"]
            .as_bool()
            .expect("validated full matrix flag"),
        prepare_only: false,
        review_output_dir: None,
    };
    verify_complete_result_bindings(result, corpus, &options, &samples)?;
    let expected_recommended =
        recommended_configuration(&options, thresholds, &samples).unwrap_or(Value::Null);
    if result.get("recommended_configuration") != Some(&expected_recommended) {
        bail!(
            "completed benchmark recommended_configuration does not match its samples and thresholds"
        );
    }

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
    let consistency = verdict_consistency(&samples);
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
    let top_three_p95 = p95(
        samples
            .iter()
            .filter(|sample| {
                sample.status == "valid" && sample.phase == "warm" && sample.candidate_count == 3
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
    let corpus_pinned = result["repositories"]
        .as_object()
        .expect("validated repositories")
        .values()
        .all(|repository| repository["matches_expected"] == true);
    let automatic_gates = corpus_pinned
        && release_matrix_complete(&options)
        && !expected_recommended.is_null()
        && runtime_phase_metrics_complete
        && structured_rate >= thresholds.structured_output_success_rate_minimum
        && rule_accuracy >= thresholds.high_severity_rule_accuracy_minimum
        && aggregate_accuracy >= thresholds.aggregate_verdict_accuracy_minimum
        && citation_completeness >= thresholds.citation_completeness_minimum
        && consistency >= thresholds.repeated_verdict_consistency_minimum
        && invalid <= thresholds.accepted_invalid_reference_maximum
        && truth_changes <= thresholds.accepted_detector_truth_change_maximum
        && abstention_recall >= thresholds.abstention_recall_minimum
        && top_one_p95
            .is_some_and(|value| value <= thresholds.warm_top_one_p95_ms_maximum)
        && top_five_p95
            .is_some_and(|value| value <= thresholds.warm_top_five_p95_ms_maximum)
        && peak_rss.is_some_and(|value| value <= thresholds.peak_process_rss_bytes_maximum)
        && swap_growth.is_some_and(|value| value <= thresholds.swap_growth_bytes_maximum);
    let expected_summary = json!({
        "sample_count": samples.len(),
        "valid_sample_count": successful,
        "structured_output_success_rate": structured_rate,
        "high_severity_rule_accuracy": rule_accuracy,
        "aggregate_verdict_accuracy": aggregate_accuracy,
        "citation_completeness": citation_completeness,
        "repeated_verdict_consistency": consistency,
        "accepted_invalid_references": invalid,
        "accepted_detector_truth_changes": truth_changes,
        "runtime_phase_metrics_complete": runtime_phase_metrics_complete,
        "abstention_recall": abstention_recall,
        "warm_top_one_p95_ms": top_one_p95,
        "warm_top_three_p95_ms": top_three_p95,
        "warm_top_five_p95_ms": top_five_p95,
        "peak_process_rss_bytes": peak_rss,
        "minimum_system_available_memory_bytes": minimum_available_memory,
        "maximum_swap_growth_bytes": swap_growth,
        "retry_count": retries,
        "automatic_gates_passed": automatic_gates,
        "corpus_report_fingerprints_pinned": corpus_pinned,
        "matrix_completed": true,
        "termination_reason": Value::Null,
    });
    let summary = result["summary"]
        .as_object()
        .expect("validated benchmark summary");
    for (key, expected) in expected_summary
        .as_object()
        .expect("expected summary object")
    {
        if summary.get(key) != Some(expected) {
            bail!("completed benchmark summary field {key:?} does not match its samples");
        }
    }
    let expected_recommendation = if successful > 0 { "adjust" } else { "defer" };
    if result.get("recommendation").and_then(Value::as_str) != Some(expected_recommendation) {
        bail!("completed unfinalized benchmark recommendation is inconsistent with its samples");
    }
    Ok(automatic_gates)
}

pub fn finalize(
    repo_root: &Path,
    corpus_path: &Path,
    thresholds_path: &Path,
    results_path: &Path,
    ratings_path: &Path,
) -> Result<PathBuf> {
    let corpus_path = resolve(repo_root, corpus_path);
    let thresholds_path = resolve(repo_root, thresholds_path);
    let results_path = resolve(repo_root, results_path);
    let ratings_path = resolve(repo_root, ratings_path);
    let corpus_bytes = read_bounded(
        &corpus_path,
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor corpus",
    )?;
    let threshold_bytes = read_bounded(
        &thresholds_path,
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor thresholds",
    )?;
    let corpus: Corpus = serde_json::from_slice(&corpus_bytes)?;
    validate_corpus(&corpus)?;
    let thresholds = parse_thresholds(&threshold_bytes)?;
    let result_bytes = read_bounded(
        &results_path,
        MAX_BENCHMARK_RESULT_BYTES,
        "advisor benchmark result",
    )?;
    let mut result: Value = serde_json::from_slice(&result_bytes)?;
    validate_benchmark_result(&result)?;
    if result.get("status").and_then(Value::as_str) != Some("complete") {
        bail!("manual ratings require a completed schema-1 advisor benchmark result");
    }
    if result.get("manual_ratings_sha256").is_some()
        || result.get("finalized_unix_ms").is_some()
    {
        bail!("advisor benchmark result is already finalized; refusing to overwrite it");
    }
    for (pointer, expected) in [
        (
            "/configuration/corpus_sha256",
            sha256(&corpus_bytes),
        ),
        (
            "/configuration/thresholds_sha256",
            sha256(&threshold_bytes),
        ),
    ] {
        if result.pointer(pointer).and_then(Value::as_str) != Some(expected.as_str()) {
            bail!("completed benchmark provenance does not match {pointer}");
        }
    }
    if result.get("thresholds") != Some(&serde_json::to_value(&thresholds)?) {
        bail!("completed benchmark thresholds do not match the preregistered thresholds file");
    }
    let automatic_passed = verify_finalization_evidence(&result, &corpus, &thresholds)?;
    let manual = ratings(Some(&ratings_path), &corpus)?.expect("ratings path is present");
    let manual_passed = manual_gates_pass(&thresholds, Some(&manual));
    let valid_samples = result
        .pointer("/summary/valid_sample_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let recommendation = if automatic_passed && manual_passed {
        "ship"
    } else if valid_samples == 0 {
        "defer"
    } else {
        "adjust"
    };
    let ratings_digest = sha256(&read_bounded(
        &ratings_path,
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor ratings",
    )?);
    let decision_path = results_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("benchmark results path has no parent"))?
        .join("decision.md");
    let original = fs::read_to_string(&decision_path)?;
    if original != render_live_decision(&result)? {
        bail!("benchmark decision report does not match its result evidence");
    }

    result["summary"]["manual_quality"] = serde_json::to_value(&manual)?;
    result["summary"]["manual_quality_gates_passed"] = json!(manual_passed);
    result["recommendation"] = json!(recommendation);
    result["manual_ratings_sha256"] = json!(ratings_digest);
    result["finalized_unix_ms"] = json!(now_ms());
    let decision = render_live_decision(&result)?;
    write_benchmark_pair(&results_path, &result, &decision_path, &decision)?;
    Ok(decision_path)
}
