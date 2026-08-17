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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BenchmarkReleaseGate {
    schema_version: u64,
    recommendation: String,
    public_inference_enabled: bool,
    canonical_model: String,
    minimum_model_size_bytes: u64,
    minimum_estimated_peak_memory_bytes: u64,
    minimum_physical_memory_bytes: u64,
    minimum_available_memory_reserve_bytes: u64,
    maximum_initial_swap_used_bytes: u64,
    maximum_swap_growth_bytes: u64,
    decision_record: String,
}

fn validate_benchmark_gate(gate: &BenchmarkReleaseGate) -> Result<()> {
    if gate.schema_version != 1
        || !matches!(gate.recommendation.as_str(), "ship" | "adjust" | "defer")
        || (gate.public_inference_enabled && gate.recommendation != "ship")
        || gate.canonical_model != "openai/gpt-oss-safeguard-20b"
        || gate.minimum_model_size_bytes < 13_793_441_254
        || gate.minimum_estimated_peak_memory_bytes < 16 * 1024 * 1024 * 1024
        || gate.minimum_estimated_peak_memory_bytes < gate.minimum_model_size_bytes
        || gate.minimum_physical_memory_bytes < 24 * 1024 * 1024 * 1024
        || gate.minimum_available_memory_reserve_bytes < 8 * 1024 * 1024 * 1024
        || gate.maximum_initial_swap_used_bytes > 256 * 1024 * 1024
        || gate.maximum_swap_growth_bytes > 256 * 1024 * 1024
        || gate.decision_record != "docs/benchmarks/safeguard-v1-m2-air-decision.md"
    {
        bail!("advisor release gate weakens the fixed V1 safety contract");
    }
    Ok(())
}

fn validate_benchmark_capacity(
    gate: &BenchmarkReleaseGate,
    model_size: u64,
    estimated_peak: u64,
    physical: u64,
    available: u64,
    swap: u64,
) -> Result<BenchmarkWatchdog> {
    let (_, _, blockers) = benchmark_capacity_blockers(
        gate,
        model_size,
        estimated_peak,
        physical,
        available,
        swap,
    )?;
    if !blockers.is_empty() {
        bail!(
            "{}",
            blockers
                .iter()
                .map(|blocker| blocker.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    Ok(BenchmarkWatchdog {
        minimum_available_memory_bytes: gate.minimum_available_memory_reserve_bytes,
        maximum_swap_growth_bytes: gate.maximum_swap_growth_bytes,
        initial_swap_used_bytes: swap,
    })
}

fn benchmark_capacity_blockers(
    gate: &BenchmarkReleaseGate,
    model_size: u64,
    estimated_peak: u64,
    physical: u64,
    available: u64,
    swap: u64,
) -> Result<(u64, u64, Vec<CapacityBlocker>)> {
    let required_available = estimated_peak
        .checked_add(gate.minimum_available_memory_reserve_bytes)
        .ok_or_else(|| anyhow::anyhow!("benchmark memory requirement overflow"))?;
    let required_physical = gate.minimum_physical_memory_bytes.max(required_available);
    let mut blockers = Vec::new();
    if model_size < gate.minimum_model_size_bytes {
        blockers.push(CapacityBlocker {
            code: "model_size_below_minimum",
            message: format!(
                "--model-size-bytes is {model_size}; the pinned canonical artifact requires at least {} bytes",
                gate.minimum_model_size_bytes
            ),
            actual_bytes: model_size,
            comparison: "minimum",
            limit_bytes: gate.minimum_model_size_bytes,
        });
    }
    if estimated_peak < gate.minimum_estimated_peak_memory_bytes {
        blockers.push(CapacityBlocker {
            code: "estimated_peak_memory_below_minimum",
            message: format!(
                "--estimated-peak-memory-bytes is {estimated_peak}; the safety floor requires at least {} bytes",
                gate.minimum_estimated_peak_memory_bytes
            ),
            actual_bytes: estimated_peak,
            comparison: "minimum",
            limit_bytes: gate.minimum_estimated_peak_memory_bytes,
        });
    }
    if estimated_peak < model_size {
        blockers.push(CapacityBlocker {
            code: "estimated_peak_memory_below_model_size",
            message: format!(
                "--estimated-peak-memory-bytes is {estimated_peak}; it cannot be smaller than the {model_size}-byte model artifact"
            ),
            actual_bytes: estimated_peak,
            comparison: "minimum",
            limit_bytes: model_size,
        });
    }
    if physical < required_physical {
        blockers.push(CapacityBlocker {
            code: "physical_memory_below_required",
            message: format!(
                "benchmark host has {physical} bytes of physical memory but requires at least {required_physical}; do not run on this host"
            ),
            actual_bytes: physical,
            comparison: "minimum",
            limit_bytes: required_physical,
        });
    }
    if available < required_available {
        blockers.push(CapacityBlocker {
            code: "available_memory_below_required",
            message: format!(
                "benchmark host has {available} bytes available but requires at least {required_available}; free capacity or use another dedicated host"
            ),
            actual_bytes: available,
            comparison: "minimum",
            limit_bytes: required_available,
        });
    }
    if swap > gate.maximum_initial_swap_used_bytes {
        blockers.push(CapacityBlocker {
            code: "initial_swap_above_maximum",
            message: format!(
                "benchmark host has {swap} bytes of swap in use but permits at most {}; recover memory pressure before any provider contact",
                gate.maximum_initial_swap_used_bytes
            ),
            actual_bytes: swap,
            comparison: "maximum",
            limit_bytes: gate.maximum_initial_swap_used_bytes,
        });
    }
    Ok((required_available, required_physical, blockers))
}

fn benchmark_safety_preflight(options: &Options) -> Result<BenchmarkWatchdog> {
    if !options.confirm_dedicated_host {
        bail!(
            "inference requires --confirm-dedicated-host; never run this matrix on a personal or capacity-constrained workstation"
        );
    }
    let gate_path = options
        .repo_root
        .join("benchmarks/advisor/release-gate.json");
    let gate: BenchmarkReleaseGate = serde_json::from_slice(&fs::read(&gate_path)?)?;
    validate_benchmark_gate(&gate)
        .with_context(|| format!("advisor release gate is invalid: {}", gate_path.display()))?;
    if options.model != gate.canonical_model {
        bail!(
            "--model must explicitly equal the release-gated canonical model {}",
            gate.canonical_model
        );
    }
    let model_size = options
        .model_size_bytes
        .ok_or_else(|| anyhow::anyhow!("inference requires explicit --model-size-bytes"))?;
    let estimated_peak = options.estimated_peak_memory_bytes.ok_or_else(|| {
        anyhow::anyhow!("inference requires explicit --estimated-peak-memory-bytes")
    })?;
    let required_available = estimated_peak
        .checked_add(gate.minimum_available_memory_reserve_bytes)
        .ok_or_else(|| anyhow::anyhow!("benchmark memory requirement overflow"))?;
    let physical = system_physical_memory_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure physical memory; refusing inference"))?;
    let available = system_available_memory_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure available memory; refusing inference"))?;
    let swap = swap_used_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure swap use; refusing inference"))?;
    let watchdog = validate_benchmark_capacity(
        &gate,
        model_size,
        estimated_peak,
        physical,
        available,
        swap,
    )?;
    eprintln!(
        "Advisor benchmark safety contract: dedicated-host-confirmed=true; model={} bytes; estimated peak={estimated_peak} bytes; required available={required_available} bytes; physical={physical} bytes; available={available} bytes; swap={swap} bytes; maximum initial swap={} bytes; maximum swap growth={} bytes; release recommendation={}; decision={}",
        model_size,
        gate.maximum_initial_swap_used_bytes,
        gate.maximum_swap_growth_bytes,
        gate.recommendation,
        gate.decision_record,
    );
    Ok(watchdog)
}

pub fn check_capacity(options: &CapacityCheckOptions) -> Result<CapacityCheck> {
    let gate_path = options
        .repo_root
        .join("benchmarks/advisor/release-gate.json");
    let gate: BenchmarkReleaseGate = serde_json::from_slice(&fs::read(&gate_path)?)?;
    validate_benchmark_gate(&gate)
        .with_context(|| format!("advisor release gate is invalid: {}", gate_path.display()))?;
    if options.model != gate.canonical_model {
        bail!(
            "--model must explicitly equal the release-gated canonical model {}",
            gate.canonical_model
        );
    }
    let physical = system_physical_memory_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure physical memory; capacity is unknown"))?;
    let available = system_available_memory_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure available memory; capacity is unknown"))?;
    let swap = swap_used_bytes()
        .ok_or_else(|| anyhow::anyhow!("unable to measure swap use; capacity is unknown"))?;
    let (required_available, required_physical, blockers) = benchmark_capacity_blockers(
        &gate,
        options.model_size_bytes,
        options.estimated_peak_memory_bytes,
        physical,
        available,
        swap,
    )?;
    let eligible = blockers.is_empty();
    Ok(CapacityCheck {
        eligible,
        receipt: json!({
            "schema_version": 1,
            "command": "advisor-capacity",
            "eligible": eligible,
            "provider_contacted": false,
            "report_accessed": false,
            "model": options.model,
            "model_size_bytes": options.model_size_bytes,
            "estimated_peak_memory_bytes": options.estimated_peak_memory_bytes,
            "required_available_memory_bytes": required_available,
            "required_physical_memory_bytes": required_physical,
            "host": {
                "physical_memory_bytes": physical,
                "available_memory_bytes": available,
                "swap_used_bytes": swap,
            },
            "limits": {
                "minimum_model_size_bytes": gate.minimum_model_size_bytes,
                "minimum_estimated_peak_memory_bytes": gate.minimum_estimated_peak_memory_bytes,
                "minimum_physical_memory_bytes": gate.minimum_physical_memory_bytes,
                "minimum_available_memory_reserve_bytes": gate.minimum_available_memory_reserve_bytes,
                "maximum_initial_swap_used_bytes": gate.maximum_initial_swap_used_bytes,
                "maximum_swap_growth_bytes": gate.maximum_swap_growth_bytes,
            },
            "release": {
                "recommendation": gate.recommendation,
                "public_inference_enabled": gate.public_inference_enabled,
                "decision_record": gate.decision_record,
            },
            "blockers": blockers,
        }),
    })
}

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
                        phase: if first && options.initial_runtime_state == "cold" {
                            "cold"
                        } else {
                            "warm"
                        },
                        status: if sample_valid { "valid" } else { "failed" },
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
                        accepted_invalid_references: 0,
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
                    });
                    first = false;
                    if monitored.termination_reason.is_some() {
                        eprintln!(
                            "Stopping the advisor matrix immediately after a continuous safety guard aborted a sample."
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
                            monitored.termination_reason,
                        );
                    }
                    let terminal_identity_failure = samples
                        .last()
                        .and_then(|sample| sample.failure_category.as_deref())
                        .filter(|category| {
                            is_terminal_provider_identity_failure(Some(category))
                        });
                    if let Some(category) = terminal_identity_failure {
                        eprintln!(
                            "Stopping the advisor matrix after provider identity drift ({category}); do not retry or accept evidence from this runtime."
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
                            Some(category),
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
