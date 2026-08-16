fn prepare_review_directory(options: &Options) -> Result<Option<PathBuf>> {
    let Some(requested) = options.review_output_dir.as_deref() else {
        return Ok(None);
    };
    if options.prepare_only {
        bail!("--review-output-dir is available only for inference runs");
    }
    if !requested.is_absolute() {
        bail!("--review-output-dir must be an absolute private path outside the repository");
    }
    if requested
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("--review-output-dir must not be a symbolic link");
    }
    let resolved = if requested.exists() {
        fs::canonicalize(requested)?
    } else {
        let parent = requested.parent().ok_or_else(|| {
            anyhow::anyhow!("--review-output-dir must have an existing parent directory")
        })?;
        fs::canonicalize(parent)?.join(
            requested
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("invalid --review-output-dir"))?,
        )
    };
    if resolved.starts_with(fs::canonicalize(&options.repo_root)?) {
        bail!("--review-output-dir must remain outside the public repository");
    }
    if resolved.exists() {
        if !resolved.is_dir() || fs::read_dir(&resolved)?.next().is_some() {
            bail!("--review-output-dir must be a new or empty directory");
        }
    } else {
        fs::create_dir(&resolved)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&resolved, fs::Permissions::from_mode(0o700))?;
    }
    eprintln!(
        "Writing explicitly requested private review artifacts outside the repository; do not commit or share their contents."
    );
    Ok(Some(resolved))
}

fn write_review_artifact(directory: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let path = directory.join(name);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(&path)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn run(options: &Options) -> Result<(PathBuf, PathBuf)> {
    if options.repetitions == 0 || options.repetitions > 20 {
        bail!("--repetitions must be between 1 and 20");
    }
    if !["ollama", "openai-compatible"].contains(&options.provider.as_str()) {
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
    if !loopback {
        bail!("the benchmark harness requires a loopback endpoint");
    }
    let binary = resolve(&options.repo_root, &options.binary);
    if !binary.is_file() {
        bail!(
            "benchmark binary is missing: {}; build it with `cargo build --release`",
            binary.display()
        );
    }
    let corpus: Corpus =
        serde_json::from_slice(&fs::read(resolve(&options.repo_root, &options.corpus))?)?;
    validate_corpus(&corpus)?;
    let thresholds: Thresholds =
        serde_json::from_slice(&fs::read(resolve(&options.repo_root, &options.thresholds))?)?;
    if thresholds.schema_version != 1 || !thresholds.preregistered_before_final_corpus {
        bail!("benchmark thresholds must use preregistered schema 1");
    }
    if options.runtime_label.len() > 100
        || options.runtime_label.is_empty()
        || options.runtime_label.chars().any(|character| {
            !(character.is_ascii_alphanumeric()
                || character.is_ascii_whitespace()
                || "._+-()".contains(character))
        })
    {
        bail!("--runtime-label must be a short privacy-safe runtime name and version");
    }
    let immutable_model_digest = options
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
    let reports = prepare_reports(&binary, &repositories, &corpus, &temporary.path)?;
    if options.prepare_only {
        return write_preflight(options, &corpus, &reports);
    }
    let efforts: &[&str] = if options.full_matrix {
        &["low", "medium", "high"]
    } else {
        &["medium"]
    };
    let started = now_ms();
    let mut samples = Vec::new();
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
        let contexts: &[usize] = if options.full_matrix {
            match case.candidate_count {
                1 => &[2_048, 4_096, 8_192],
                2 | 3 => &[4_096, 8_192],
                _ => &[8_192],
            }
        } else {
            &[8_192]
        };
        for effort in efforts {
            for context in contexts {
                for repetition in 1..=options.repetitions {
                    if first {
                        if let Some(model) = &options.ollama_cold_model {
                            let status = Command::new("ollama").args(["stop", model]).status()?;
                            if !status.success() {
                                bail!("ollama cold-start reset failed for {model}");
                            }
                        }
                    }
                    let artifact_path = std::env::temp_dir().join(format!(
                        "git-slop-advisor-benchmark-{}-{}-{}-{}.json",
                        std::process::id(),
                        case.id,
                        effort,
                        repetition
                    ));
                    let output_token_limit = case.candidate_count.saturating_mul(2_048).min(8_192);
                    let mut args = vec![
                        "--repo".to_string(),
                        repository.display().to_string(),
                        "advise".to_string(),
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
                        "--runtime-model".to_string(),
                        options.runtime_model.clone(),
                        "--runtime-label".to_string(),
                        options.runtime_label.clone(),
                        "--model-digest".to_string(),
                        options.model_digest.clone(),
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
                    let (output, rss) = timed_output(&binary, &args, repository)?;
                    let elapsed = wall.elapsed().as_millis();
                    let available_after = system_available_memory_bytes();
                    let swap_after = swap_used_bytes();
                    let artifact_bytes = output
                        .status
                        .success()
                        .then(|| fs::read(&artifact_path).ok())
                        .flatten();
                    let artifact = artifact_bytes
                        .as_deref()
                        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
                    let _ = fs::remove_file(&artifact_path);
                    let actual_candidate_count = artifact.as_ref().and_then(|artifact| {
                        artifact
                            .pointer("/evaluation/candidate_evaluations")
                            .and_then(Value::as_array)
                            .map(Vec::len)
                    });
                    let sample_valid =
                        artifact.is_some() && actual_candidate_count == Some(case.candidate_count);
                    if sample_valid && *context == 8_192 && repetition == 1 {
                        if let (Some(directory), Some(bytes)) =
                            (review_directory.as_deref(), artifact_bytes.as_deref())
                        {
                            write_review_artifact(
                                directory,
                                &format!("{}-{effort}-8192.json", case.id),
                                bytes,
                            )?;
                        }
                    }
                    let (matched, total) = artifact
                        .as_ref()
                        .map(|artifact| rule_scores(artifact, &case.expected_rule_verdicts))
                        .unwrap_or((
                            0,
                            high_severity_expectation_count(&case.expected_rule_verdicts)
                                * case.candidate_count,
                        ));
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
                    samples.push(Sample {
                        case_id: case.id.clone(),
                        repository: case.repository.clone(),
                        scenario_tags: case.scenario_tags.clone(),
                        scenario: case.scenario.clone(),
                        candidate_count: case.candidate_count,
                        actual_candidate_count,
                        report_sha256: report.sha256.clone(),
                        reasoning_effort: (*effort).to_string(),
                        context_token_limit: *context,
                        output_token_limit,
                        repetition,
                        phase: if first { "cold" } else { "warm" },
                        status: if sample_valid { "valid" } else { "failed" },
                        exit_code: output.status.code(),
                        total_elapsed_ms: elapsed,
                        peak_process_rss_bytes: rss,
                        system_available_memory_before_bytes: available_before,
                        system_available_memory_after_bytes: available_after,
                        system_available_memory_minimum_bytes: match (
                            available_before,
                            available_after,
                        ) {
                            (Some(before), Some(after)) => Some(before.min(after)),
                            (Some(value), None) | (None, Some(value)) => Some(value),
                            (None, None) => None,
                        },
                        swap_before_bytes: swap_before,
                        swap_after_bytes: swap_after,
                        swap_growth_bytes: swap_before
                            .zip(swap_after)
                            .map(|(before, after)| after.saturating_sub(before)),
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
                        accepted_invalid_references: 0,
                        accepted_detector_truth_changes: artifact.as_ref().map_or(0, |artifact| {
                            accepted_detector_truth_changes(artifact, &case.scenario)
                        }),
                        citation_complete: artifact.as_ref().is_some_and(citations_complete),
                        retry_count: 0,
                        failure_category: if artifact.is_none() {
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
                    });
                    first = false;
                    consecutive_provider_failures = if is_provider_runtime_failure(
                        samples
                            .last()
                            .and_then(|sample| sample.failure_category.as_deref()),
                    ) {
                        consecutive_provider_failures.saturating_add(1)
                    } else {
                        0
                    };
                    if consecutive_provider_failures
                        >= BENCHMARK_CONSECUTIVE_PROVIDER_FAILURE_LIMIT
                    {
                        eprintln!(
                            "Stopping the advisor matrix after {consecutive_provider_failures} consecutive provider/runtime failures; writing a fail-closed incomplete result."
                        );
                        return write_outputs(
                            options,
                            &OutputInputs {
                                corpus: &corpus,
                                reports: &reports,
                                thresholds: &thresholds,
                            },
                            started,
                            &samples,
                            None,
                            Some("consecutive_provider_runtime_failures"),
                        );
                    }
                }
            }
        }
    }
    let manual = ratings(options.ratings.as_deref(), &corpus)?;
    write_outputs(
        options,
        &OutputInputs {
            corpus: &corpus,
            reports: &reports,
            thresholds: &thresholds,
        },
        started,
        &samples,
        manual.as_ref(),
        None,
    )
}
