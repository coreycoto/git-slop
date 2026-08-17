pub fn run(options: &Options) -> Result<(PathBuf, PathBuf)> {
    if options.repetitions == 0 || options.repetitions > 20 {
        bail!("--repetitions must be between 1 and 20");
    }
    if !options.prepare_only
        && !["ollama", "openai-compatible"].contains(&options.provider.as_str())
    {
        bail!("the benchmark provider must be ollama or openai-compatible");
    }
    let authority = options
        .endpoint
        .strip_prefix("http://")
        .and_then(|value| value.split('/').next());
    let loopback = authority.is_some_and(|value| {
        value == "localhost"
            || value.starts_with("localhost:")
            || value == "127.0.0.1"
            || value.starts_with("127.0.0.1:")
            || value == "[::1]"
            || value.starts_with("[::1]:")
    });
    if !options.prepare_only && !loopback {
        bail!("the benchmark harness requires a loopback endpoint");
    }
    let watchdog = (!options.prepare_only)
        .then(|| benchmark_safety_preflight(options))
        .transpose()?;
    let binary = resolve(&options.repo_root, &options.binary);
    if !binary.is_file() {
        bail!(
            "benchmark binary is missing: {}; build it with `cargo build --release`",
            binary.display()
        );
    }
    let provenance = collect_benchmark_provenance(options, &binary)?;
    let corpus_bytes = read_bounded(
        &resolve(&options.repo_root, &options.corpus),
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor corpus",
    )?;
    let corpus: Corpus = serde_json::from_slice(&corpus_bytes)?;
    validate_corpus(&corpus)?;
    let threshold_bytes = read_bounded(
        &resolve(&options.repo_root, &options.thresholds),
        MAX_BENCHMARK_CONFIG_BYTES,
        "advisor thresholds",
    )?;
    let thresholds = parse_thresholds(&threshold_bytes)?;
    if options.runtime_label.len() > 100
        || options.runtime_label.is_empty()
        || !options
            .runtime_label
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || options.runtime_label.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || character == ' ' || "._+-()".contains(character))
        })
    {
        bail!("--runtime-label must be a short privacy-safe runtime name and version");
    }
    if !privacy_safe_benchmark_runtime_identifier(&options.runtime_model) {
        bail!("--runtime-model must be a privacy-safe runtime identifier");
    }
    let immutable_model_digest =
        options
            .model_digest
            .strip_prefix("sha256:")
            .is_some_and(|digest| {
                digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
    if (!options.prepare_only && !immutable_model_digest)
        || (options.prepare_only && options.model_digest != "not-applicable")
    {
        bail!(
            "--model-digest must be sha256:<64 hex characters> for inference, or not-applicable with --prepare-only"
        );
    }
    let quantization_is_safe = !options.model_quantization.is_empty()
        && options.model_quantization.len() <= 64
        && options
            .model_quantization
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte));
    if !quantization_is_safe
        || (!options.prepare_only && options.model_quantization == "not-applicable")
        || (options.prepare_only && options.model_quantization != "not-applicable")
    {
        bail!(
            "--model-quantization must be the runtime-reported quantization for inference, or not-applicable with --prepare-only"
        );
    }
    let repositories = repository_map(&options.repositories, &corpus)?;
    let review_directory = prepare_review_directory(options)?;
    let temporary = TemporaryWorkspace::new()?;
    let reports = prepare_reports(&binary, &repositories, &corpus, temporary.path())?;
    if options.prepare_only {
        return write_preflight(options, &corpus, &reports, &provenance);
    }
    let sample_artifacts = temporary.path().join("sample-artifacts");
    fs::create_dir(&sample_artifacts)?;
    let efforts = expected_efforts(options.full_matrix);
    let started = now_ms();
    let mut samples = Vec::new();
    let mut review_entries = Vec::new();
    let mut first = true;
    let mut consecutive_provider_failures = 0usize;
    for case in &corpus.cases {
        let repository = repositories.get(&case.repository).ok_or_else(|| {
            anyhow::anyhow!(
                "corpus case {} requires --repository {}=PATH",
                case.id,
                case.repository
            )
        })?;
        let report = reports
            .get(&case.repository)
            .expect("mapped repository has a prepared report");
        let contexts = expected_contexts(options.full_matrix, case.candidate_count);
        for effort in efforts {
            for context in contexts {
                for repetition in 1..=options.repetitions {
                    let artifact_path = sample_artifacts.join(format!(
                        "{}-{}-{}-{}.json",
                        case.id, effort, context, repetition
                    ));
                    if artifact_path.exists() {
                        bail!(
                            "refusing to reuse benchmark sample artifact {}",
                            artifact_path.display()
                        );
                    }
                    let output_token_limit = case.candidate_count.saturating_mul(2_048).min(8_192);
                    let mut args = vec![
                        "--repo".to_string(),
                        repository.display().to_string(),
                        "--error-format".to_string(),
                        "json".to_string(),
                        "advise".to_string(),
                        "--infer".to_string(),
                    ];
                    args.extend(case.selector.iter().cloned());
                    args.extend([
                        "--report".to_string(),
                        report.path.display().to_string(),
                        "--ephemeral".to_string(),
                        "--evaluation-scenario".to_string(),
                        case.scenario.clone(),
                        "--provider".to_string(),
                        options.provider.clone(),
                        "--endpoint".to_string(),
                        options.endpoint.clone(),
                        "--model".to_string(),
                        options.model.clone(),
                        "--runtime-model".to_string(),
                        options.runtime_model.clone(),
                        "--runtime-label".to_string(),
                        options.runtime_label.clone(),
                        "--model-digest".to_string(),
                        options.model_digest.clone(),
                        "--model-size-bytes".to_string(),
                        options
                            .model_size_bytes
                            .expect("inference preflight requires model size")
                            .to_string(),
                        "--estimated-peak-memory-bytes".to_string(),
                        options
                            .estimated_peak_memory_bytes
                            .expect("inference preflight requires peak estimate")
                            .to_string(),
                        "--confirm-resources".to_string(),
                        "--reasoning".to_string(),
                        (*effort).to_string(),
                        "--max-context-tokens".to_string(),
                        context.to_string(),
                        "--max-output-tokens".to_string(),
                        output_token_limit.to_string(),
                        "--runtime-context-tokens".to_string(),
                        BENCHMARK_RUNTIME_CONTEXT_TOKENS.to_string(),
                        "--timeout-seconds".to_string(),
                        BENCHMARK_TIMEOUT_SECONDS.to_string(),
                        "--format".to_string(),
                        "json".to_string(),
                        "--output".to_string(),
                        artifact_path.display().to_string(),
                    ]);
                    let swap_before = swap_used_bytes();
                    let available_before = system_available_memory_bytes();
                    let wall = Instant::now();
                    let monitored = timed_output(
                        &binary,
                        &args,
                        repository,
                        watchdog.expect("inference preflight returns watchdog limits"),
                    )?;
                    let output = monitored.output;
                    let elapsed = wall.elapsed().as_millis();
                    let available_after = system_available_memory_bytes();
                    let swap_after = swap_used_bytes();
                    let artifact_bytes = artifact_path.exists().then(|| {
                        read_bounded(
                            &artifact_path,
                            MAX_BENCHMARK_CHILD_ARTIFACT_BYTES,
                            "benchmark child advice artifact",
                        )
                    });
                    let artifact_bytes = artifact_bytes.transpose()?;
                    let artifact = artifact_bytes
                        .as_deref()
                        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
                    let assessment = artifact
                        .as_ref()
                        .map(|artifact| {
                            assess_advice_artifact(artifact, report, case.candidate_count)
                        })
                        .transpose()?;
                    let actual_candidate_count = assessment
                        .as_ref()
                        .and_then(|assessment| assessment.actual_candidate_count);
                    let sample_valid = output.status.success()
                        && assessment
                            .as_ref()
                            .is_some_and(|assessment| assessment.valid);
                    let invalid_references = assessment
                        .as_ref()
                        .map_or(0, |assessment| assessment.invalid_references);
                    let (matched, _) = artifact
                        .as_ref()
                        .map(|artifact| rule_scores(artifact, &case.expected_rule_verdicts))
                        .unwrap_or((0, 0));
                    let total = high_severity_expectation_count(&case.expected_rule_verdicts)
                        * case.candidate_count;
                    let aggregate = artifact
                        .as_ref()
                        .and_then(|artifact| artifact.pointer("/evaluation/aggregate_verdict"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let input_tokens = artifact.as_ref().and_then(|artifact| {
                        u64_at(artifact, "/provider/usage/prompt_tokens").or_else(|| {
                            u64_at(artifact, "/provider/runtime_timings/prompt_eval_count")
                        })
                    });
                    let output_tokens = artifact.as_ref().and_then(|artifact| {
                        u64_at(artifact, "/provider/usage/completion_tokens")
                            .or_else(|| u64_at(artifact, "/provider/runtime_timings/eval_count"))
                    });
                    let prompt_duration = artifact.as_ref().and_then(|artifact| {
                        u64_at(artifact, "/provider/runtime_timings/prompt_eval_duration")
                    });
                    let generation_duration = artifact.as_ref().and_then(|artifact| {
                        u64_at(artifact, "/provider/runtime_timings/eval_duration")
                    });
                    let sample = seal_sample(Sample {
                        case_id: case.id.clone(),
                        repository: case.repository.clone(),
                        scenario_tags: case.scenario_tags.clone(),
                        scenario: case.scenario.clone(),
                        candidate_count: case.candidate_count,
                        actual_candidate_count,
                        report_sha256: report.sha256.clone(),
                        artifact_sha256: artifact_bytes.as_deref().map(sha256),
                        sample_sha256: String::new(),
                        reasoning_effort: (*effort).to_string(),
                        context_token_limit: *context,
                        output_token_limit,
                        repetition,
                        phase: if first && options.initial_runtime_state == "cold" {
                            "cold".to_string()
                        } else {
                            "warm".to_string()
                        },
                        status: if sample_valid { "valid" } else { "failed" }.to_string(),
                        exit_code: output.status.code(),
                        total_elapsed_ms: elapsed,
                        peak_process_rss_bytes: monitored.peak_process_rss_bytes,
                        system_available_memory_before_bytes: available_before,
                        system_available_memory_after_bytes: available_after,
                        system_available_memory_minimum_bytes: [
                            monitored.minimum_available_memory_bytes,
                            available_before,
                            available_after,
                        ]
                        .into_iter()
                        .flatten()
                        .min(),
                        swap_before_bytes: swap_before,
                        swap_after_bytes: swap_after,
                        swap_growth_bytes: [
                            monitored.maximum_swap_growth_bytes,
                            swap_before
                                .zip(swap_after)
                                .map(|(before, after)| after.saturating_sub(before)),
                        ]
                        .into_iter()
                        .flatten()
                        .max(),
                        context_elapsed_ms: artifact
                            .as_ref()
                            .and_then(|value| u64_at(value, "/timing/context_elapsed_ms")),
                        provider_elapsed_ms: artifact
                            .as_ref()
                            .and_then(|value| u64_at(value, "/timing/provider_elapsed_ms")),
                        validation_elapsed_ms: artifact
                            .as_ref()
                            .and_then(|value| u64_at(value, "/timing/validation_elapsed_ms")),
                        time_to_validated_artifact_ms: artifact.as_ref().and_then(|value| {
                            u64_at(value, "/timing/time_to_validated_artifact_ms")
                        }),
                        model_load_duration_ns: artifact.as_ref().and_then(|value| {
                            u64_at(value, "/provider/runtime_timings/load_duration")
                        }),
                        prompt_eval_duration_ns: prompt_duration,
                        generation_duration_ns: generation_duration,
                        input_tokens,
                        output_tokens,
                        prompt_tokens_per_second: rate(input_tokens, prompt_duration),
                        output_tokens_per_second: rate(output_tokens, generation_duration),
                        reported_aggregate: aggregate.clone(),
                        expected_aggregate: case.expected_aggregate.clone(),
                        aggregate_match: aggregate.as_deref()
                            == Some(case.expected_aggregate.as_str()),
                        matched_rule_verdicts: matched,
                        expected_rule_verdicts: total,
                        accepted_invalid_references: invalid_references,
                        accepted_detector_truth_changes: artifact.as_ref().map_or(0, |artifact| {
                            accepted_detector_truth_changes(artifact, &case.scenario)
                        }),
                        citation_complete: artifact.as_ref().is_some_and(citations_complete),
                        retry_count: 0,
                        failure_category: if let Some(reason) = monitored.termination_reason {
                            Some(reason.to_string())
                        } else if artifact.is_none() {
                            Some(classify_failure(&output.stderr, false))
                        } else if !sample_valid {
                            Some(if actual_candidate_count == Some(case.candidate_count) {
                                classify_failure(&output.stderr, true)
                            } else {
                                "corpus_candidate_count_drift".to_string()
                            })
                        } else {
                            None
                        },
                    })?;
                    samples.push(sample);
                    if sample_valid && *context == 8_192 {
                        if let (Some(directory), Some(artifact)) =
                            (review_directory.as_deref(), artifact.as_ref())
                        {
                            record_review_artifact(
                                directory,
                                &mut review_entries,
                                samples.last().expect("sample was just recorded"),
                                artifact,
                            )?;
                        }
                    }
                    first = false;
                    write_outputs(
                        options,
                        &OutputInputs {
                            corpus: &corpus,
                            reports: &reports,
                            thresholds: &thresholds,
                            provenance: &provenance,
                        },
                        started,
                        &samples,
                        None,
                        monitored
                            .termination_reason
                            .or(Some("benchmark_checkpoint")),
                    )?;
                    if artifact_path.exists() {
                        fs::remove_file(&artifact_path).with_context(|| {
                            format!(
                                "failed to remove private benchmark sample artifact {}",
                                artifact_path.display()
                            )
                        })?;
                    }
                    if monitored.termination_reason.is_some() {
                        eprintln!(
                            "Stopping the advisor matrix immediately after a continuous safety guard aborted a sample."
                        );
                        return write_terminal_outputs(
                            options,
                            &OutputInputs {
                                corpus: &corpus,
                                reports: &reports,
                                thresholds: &thresholds,
                                provenance: &provenance,
                            },
                            started,
                            &samples,
                            monitored.termination_reason,
                            review_directory.as_deref(),
                            &review_entries,
                        );
                    }
                    let terminal_identity_failure = samples
                        .last()
                        .and_then(|sample| sample.failure_category.as_deref())
                        .filter(|category| is_terminal_provider_identity_failure(Some(category)));
                    if let Some(category) = terminal_identity_failure {
                        eprintln!(
                            "Stopping the advisor matrix after provider identity drift ({category}); do not retry or accept evidence from this runtime."
                        );
                        return write_terminal_outputs(
                            options,
                            &OutputInputs {
                                corpus: &corpus,
                                reports: &reports,
                                thresholds: &thresholds,
                                provenance: &provenance,
                            },
                            started,
                            &samples,
                            Some(category),
                            review_directory.as_deref(),
                            &review_entries,
                        );
                    }
                    consecutive_provider_failures = if is_provider_runtime_failure(
                        samples
                            .last()
                            .and_then(|sample| sample.failure_category.as_deref()),
                    ) {
                        consecutive_provider_failures.saturating_add(1)
                    } else {
                        0
                    };
                    if consecutive_provider_failures >= BENCHMARK_CONSECUTIVE_PROVIDER_FAILURE_LIMIT
                    {
                        eprintln!(
                            "Stopping the advisor matrix after {consecutive_provider_failures} consecutive provider/runtime failures; writing a fail-closed incomplete result."
                        );
                        return write_terminal_outputs(
                            options,
                            &OutputInputs {
                                corpus: &corpus,
                                reports: &reports,
                                thresholds: &thresholds,
                                provenance: &provenance,
                            },
                            started,
                            &samples,
                            Some("consecutive_provider_runtime_failures"),
                            review_directory.as_deref(),
                            &review_entries,
                        );
                    }
                }
            }
        }
    }
    write_terminal_outputs(
        options,
        &OutputInputs {
            corpus: &corpus,
            reports: &reports,
            thresholds: &thresholds,
            provenance: &provenance,
        },
        started,
        &samples,
        None,
        review_directory.as_deref(),
        &review_entries,
    )
}
