use super::*;

struct VerifiedFinalizationEvidence {
    options: Options,
    samples: Vec<Sample>,
    corpus_pinned: bool,
}

fn finalization_options(result: &Value) -> Options {
    let configuration = &result["configuration"];
    Options {
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
    }
}

fn verify_finalization_evidence(
    result: &Value,
    corpus: &Corpus,
    thresholds: &Thresholds,
) -> Result<VerifiedFinalizationEvidence> {
    let samples: Vec<Sample> = serde_json::from_value(
        result
            .get("samples")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("completed benchmark result is missing samples"))?,
    )
    .context("completed benchmark samples are invalid")?;
    let options = finalization_options(result);
    verify_complete_result_bindings(result, corpus, &options, &samples)?;
    let corpus_pinned = result["repositories"]
        .as_object()
        .expect("validated repositories")
        .values()
        .all(|repository| repository["matches_expected"] == true);
    let derivation = derive_benchmark(&options, thresholds, &samples, corpus_pinned, None, None)?;
    if result.get("recommended_configuration")
        != Some(
            &derivation
                .recommended_configuration
                .clone()
                .unwrap_or(Value::Null),
        )
    {
        bail!(
            "completed benchmark recommended_configuration does not match its samples and thresholds"
        );
    }
    let recorded_summary = result["summary"].as_object().expect("validated summary");
    for (field, expected) in derivation.summary.as_object().expect("derived summary") {
        if recorded_summary.get(field) != Some(expected) {
            bail!(
                "completed benchmark summary field {field:?} does not match its samples and thresholds"
            );
        }
    }
    if result.get("recommendation") != Some(&serde_json::to_value(derivation.recommendation)?) {
        bail!("completed unfinalized benchmark recommendation is inconsistent with its samples");
    }
    Ok(VerifiedFinalizationEvidence {
        options,
        samples,
        corpus_pinned,
    })
}

pub fn finalize(options: &FinalizeOptions) -> Result<FinalizeOutcome> {
    let corpus_path = resolve(&options.repo_root, &options.corpus);
    let thresholds_path = resolve(&options.repo_root, &options.thresholds);
    let results_path = resolve(&options.repo_root, &options.results);
    let review_manifest_path = resolve(&options.repo_root, &options.review_manifest);
    let ratings_path = resolve(&options.repo_root, &options.ratings);
    let output_path = resolve(&options.repo_root, &options.output);
    let decision_output_path = resolve(&options.repo_root, &options.decision_output);
    if output_path == results_path
        || decision_output_path == results_path
        || output_path == decision_output_path
    {
        bail!("finalized outputs must be new files distinct from the immutable source result");
    }
    if output_path.exists() || decision_output_path.exists() {
        bail!("finalized output already exists; refusing to overwrite immutable evidence");
    }
    let corpus_bytes = read_bounded(&corpus_path, MAX_BENCHMARK_CONFIG_BYTES, "advisor corpus")?;
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
    let source_results_sha256 = sha256(&result_bytes);
    let mut result: Value = serde_json::from_slice(&result_bytes)?;
    validate_benchmark_result(&result)?;
    if result.get("status").and_then(Value::as_str) != Some("complete") {
        bail!("manual ratings require a completed schema-1 advisor benchmark result");
    }
    if result.get("manual_ratings_sha256").is_some()
        || result.get("source_results_sha256").is_some()
        || result.get("review_manifest_sha256").is_some()
        || result.get("finalized_unix_ms").is_some()
    {
        bail!("advisor benchmark result is already finalized; refusing to overwrite it");
    }
    for (pointer, expected) in [
        ("/configuration/corpus_sha256", sha256(&corpus_bytes)),
        ("/configuration/thresholds_sha256", sha256(&threshold_bytes)),
    ] {
        if result.pointer(pointer).and_then(Value::as_str) != Some(expected.as_str()) {
            bail!("completed benchmark provenance does not match {pointer}");
        }
    }
    if result.get("thresholds") != Some(&serde_json::to_value(&thresholds)?) {
        bail!("completed benchmark thresholds do not match the preregistered thresholds file");
    }
    let evidence = verify_finalization_evidence(&result, &corpus, &thresholds)?;
    let (review_manifest, review_manifest_sha256) =
        load_review_manifest(&review_manifest_path, &source_results_sha256)?;
    let review_ids = selected_review_ids(&result, &evidence.samples, &review_manifest)?;
    let manual = ratings(
        &ratings_path,
        &source_results_sha256,
        &review_manifest_sha256,
        &review_ids,
    )?;
    let finalized = derive_benchmark(
        &evidence.options,
        &thresholds,
        &evidence.samples,
        evidence.corpus_pinned,
        Some(&manual),
        None,
    )?;
    let ratings_digest = sha256(&read_bounded(
        &ratings_path,
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor ratings",
    )?);
    let source_decision_path = results_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("benchmark results path has no parent"))?
        .join("decision.md");
    let original = fs::read_to_string(&source_decision_path)?;
    if original != render_live_decision(&result)? {
        bail!("benchmark decision report does not match its result evidence");
    }

    result["summary"] = finalized.summary;
    result["recommendation"] = serde_json::to_value(finalized.recommendation)?;
    result["source_results_sha256"] = json!(source_results_sha256);
    result["review_manifest_sha256"] = json!(review_manifest_sha256);
    result["manual_ratings_sha256"] = json!(ratings_digest);
    result["finalized_unix_ms"] = json!(now_ms());
    validate_benchmark_result(&result)?;
    let decision = render_live_decision(&result)?;
    let proposed_results = serde_json::to_string_pretty(&result)? + "\n";
    let operation_code = if options.apply {
        write_benchmark_pair(
            &output_path,
            &result,
            &decision_output_path,
            &decision,
            false,
        )?;
        "advisor_benchmark_finalize_applied"
    } else {
        "advisor_benchmark_finalize_preview_valid"
    };
    let receipt = json!({
        "schema_version": 1,
        "operation": "advisor-benchmark-finalize",
        "operation_code": operation_code,
        "status": if options.apply { "applied" } else { "preview" },
        "apply": options.apply,
        "recommendation": finalized.recommendation,
        "source_results_sha256": result["source_results_sha256"],
        "review_manifest_sha256": result["review_manifest_sha256"],
        "manual_ratings_sha256": result["manual_ratings_sha256"],
        "proposed_results_sha256": sha256(proposed_results.as_bytes()),
        "results_output": output_path,
        "decision_output": decision_output_path
    });
    validate_operation_receipt(&receipt)?;
    Ok(FinalizeOutcome {
        receipt,
        results_path: output_path,
        decision_path: decision_output_path,
    })
}
