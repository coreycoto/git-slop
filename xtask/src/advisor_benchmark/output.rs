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
            "model": "openai/gpt-oss-safeguard-20b", "model_digest": options.model_digest,
            "model_quantization": options.model_quantization,
            "runtime_context_tokens": BENCHMARK_RUNTIME_CONTEXT_TOKENS,
            "request_timeout_seconds": BENCHMARK_TIMEOUT_SECONDS,
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
        result.as_object_mut().expect("benchmark result is an object").insert(
            "next_step".to_string(),
            json!("Reduce local runtime memory pressure or compare another existing Apple Silicon runtime, then rerun the pinned matrix from clean checkouts."),
        );
    }
    let output_dir = resolve(&options.repo_root, &options.output_dir);
    fs::create_dir_all(&output_dir)?;
    let json_path = output_dir.join("results.json");
    fs::write(&json_path, serde_json::to_string_pretty(&result)? + "\n")?;
    let markdown_path = output_dir.join("decision.md");
    let recommended_text = recommended.as_ref().map_or_else(
        || "unavailable; no default configuration passed every automatic quality and performance gate".to_string(),
        |value| format!(
            "{} via {} with {} reasoning and an {}-token default context",
            value["runtime_label"].as_str().unwrap_or("unknown runtime"),
            value["provider"].as_str().unwrap_or("unknown provider"),
            value["reasoning_effort"].as_str().unwrap_or("unknown"),
            value["max_context_tokens"].as_u64().unwrap_or_default()
        ),
    );
    let markdown = format!(
        "# Safeguard-only V1 decision\n\n- Recommendation: **{recommendation}**\n- Matrix completed: **{matrix_completed}**\n- Termination reason: {}\n- Recommended configuration: {recommended_text}\n- Model quantization: {}\n- Corpus report fingerprints pinned: **{corpus_pinned}**\n- Runtime phase metrics complete: **{runtime_phase_metrics_complete}**\n- Valid structured outputs: {successful}/{} ({structured_rate:.3})\n- High-severity rule accuracy: {rule_accuracy:.3}\n- Aggregate verdict accuracy: {aggregate_accuracy:.3}\n- Citation completeness: {citation_completeness:.3}\n- Repeated-verdict consistency: {consistency:.3}\n- Abstention recall: {abstention_recall:.3}\n- Warm top-one p95: {} ms\n- Warm top-three p95: {} ms\n- Warm top-five p95: {} ms\n- Peak process RSS: {} bytes\n- Minimum observed system-available memory: {} bytes\n- Maximum observed swap growth: {} bytes\n- Retries: {retries}\n- Maintainer usefulness mean: {}\n- Manual quality mean: {}\n- Unsupported claims found by maintainers: {}\n\nNo raw repository content, prompts, paths, rationales, or private skill content is included in this report. Inspect ephemeral advice output during the run for case-level review; it is removed automatically.\n",
        termination_reason.unwrap_or("none"),
        options.model_quantization,
        samples.len(),
        top_one_p95.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        top_three_p95.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        top_five_p95.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        peak_rss.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        minimum_available_memory
            .map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        swap_growth.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        manual.map_or_else(
            || "not reviewed".to_string(),
            |scores| format!("{:.2}", scores.recommendation_usefulness_mean)
        ),
        manual.map_or_else(
            || "not reviewed".to_string(),
            |scores| format!("{:.2}", scores.overall_quality_mean)
        ),
        manual.map_or_else(
            || "not reviewed".to_string(),
            |scores| scores.unsupported_claim_count.to_string()
        ),
    );
    fs::write(&markdown_path, markdown)?;
    Ok((json_path, markdown_path))
}

fn ratings(path: Option<&Path>, corpus: &Corpus) -> Result<Option<ManualScores>> {
    let Some(path) = path else { return Ok(None) };
    let ratings: RatingsFile = serde_json::from_slice(&fs::read(path)?)?;
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

fn write_preflight(
    options: &Options,
    corpus: &Corpus,
    reports: &BTreeMap<String, PreparedReport>,
) -> Result<(PathBuf, PathBuf)> {
    let output_dir = resolve(&options.repo_root, &options.output_dir);
    fs::create_dir_all(&output_dir)?;
    let repositories = reports
        .iter()
        .map(|(key, report)| {
            let fixture = corpus
                .repositories
                .get(key)
                .expect("prepared report has a corpus fixture");
            (
                key,
                json!({
                    "revision": fixture.revision,
                    "as_of": fixture.as_of,
                    "report_sha256": report.sha256,
                    "matches_expected": fixture.expected_report_sha256.as_deref() == Some(report.sha256.as_str())
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let result = json!({
        "schema_version": 1,
        "status": "incomplete",
        "configuration": {
            "corpus": corpus.id,
            "mode": "prepare-only",
            "corpus_sha256": sha256(&fs::read(resolve(&options.repo_root, &options.corpus))?),
            "thresholds_sha256": sha256(&fs::read(resolve(&options.repo_root, &options.thresholds))?)
        },
        "system": system_profile(),
        "repositories": repositories,
        "recommended_configuration": Value::Null,
        "recommendation": "defer",
        "next_step": "Review the deterministic candidates, then pin these report fingerprints before the live model matrix."
    });
    let json_path = output_dir.join("results.json");
    fs::write(&json_path, serde_json::to_string_pretty(&result)? + "\n")?;
    let markdown_path = output_dir.join("decision.md");
    fs::write(
        &markdown_path,
        "# Safeguard-only V1 decision\n\n- Recommendation: **defer**\n- Status: deterministic report preparation only; no model inference was attempted.\n\nReview and pin the privacy-safe report fingerprints in `results.json` before the live matrix.\n",
    )?;
    Ok((json_path, markdown_path))
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
    let corpus_bytes = fs::read(&corpus_path)?;
    let threshold_bytes = fs::read(&thresholds_path)?;
    let corpus: Corpus = serde_json::from_slice(&corpus_bytes)?;
    validate_corpus(&corpus)?;
    let thresholds: Thresholds = serde_json::from_slice(&threshold_bytes)?;
    if thresholds.schema_version != 1 || !thresholds.preregistered_before_final_corpus {
        bail!("benchmark thresholds must use preregistered schema 1");
    }
    let mut result: Value = serde_json::from_slice(&fs::read(&results_path)?)?;
    if result.get("schema_version").and_then(Value::as_u64) != Some(1)
        || result.get("status").and_then(Value::as_str) != Some("complete")
        || result.pointer("/summary/automatic_gates_passed").and_then(Value::as_bool).is_none()
    {
        bail!("manual ratings require a completed schema-1 advisor benchmark result");
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
    let manual = ratings(Some(&ratings_path), &corpus)?.expect("ratings path is present");
    let manual_passed = manual_gates_pass(&thresholds, Some(&manual));
    let automatic_passed = result
        .pointer("/summary/automatic_gates_passed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let recommendation = if automatic_passed && manual_passed {
        "ship"
    } else {
        "adjust"
    };
    result["summary"]["manual_quality"] = serde_json::to_value(&manual)?;
    result["summary"]["manual_quality_gates_passed"] = json!(manual_passed);
    result["recommendation"] = json!(recommendation);
    let ratings_digest = sha256(&fs::read(&ratings_path)?);
    result["manual_ratings_sha256"] = json!(ratings_digest);
    result["finalized_unix_ms"] = json!(now_ms());
    fs::write(
        &results_path,
        serde_json::to_string_pretty(&result)? + "\n",
    )?;

    let decision_path = results_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("benchmark results path has no parent"))?
        .join("decision.md");
    let original = fs::read_to_string(&decision_path)?;
    let mut saw_recommendation = false;
    let mut saw_usefulness = false;
    let mut saw_quality = false;
    let mut saw_unsupported = false;
    let mut lines = Vec::new();
    for line in original.lines() {
        if line.starts_with("- Recommendation: ") {
            lines.push(format!("- Recommendation: **{recommendation}**"));
            saw_recommendation = true;
        } else if line.starts_with("- Maintainer usefulness mean: ") {
            lines.push(format!(
                "- Maintainer usefulness mean: {:.2}",
                manual.recommendation_usefulness_mean
            ));
            saw_usefulness = true;
        } else if line.starts_with("- Manual quality mean: ") {
            lines.push(format!(
                "- Manual quality mean: {:.2}",
                manual.overall_quality_mean
            ));
            saw_quality = true;
        } else if line.starts_with("- Unsupported claims found by maintainers: ") {
            lines.push(format!(
                "- Unsupported claims found by maintainers: {}",
                manual.unsupported_claim_count
            ));
            lines.push(format!("- Manual ratings SHA-256: {ratings_digest}"));
            saw_unsupported = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !(saw_recommendation && saw_usefulness && saw_quality && saw_unsupported) {
        bail!("benchmark decision report is missing a required finalization field");
    }
    fs::write(&decision_path, lines.join("\n") + "\n")?;
    Ok(decision_path)
}
