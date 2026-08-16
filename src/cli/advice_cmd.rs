fn advice_selector(args: &AdviseArgs, repo_root: &Path) -> crate::advice::AdviceSelector {
    if let Some(path) = &args.path {
        crate::advice::AdviceSelector::Path(selector_path(repo_root, path))
    } else if let Some(id) = &args.relationship {
        crate::advice::AdviceSelector::Relationship(id.clone())
    } else if let Some(id) = &args.cluster {
        crate::advice::AdviceSelector::Cluster(id.clone())
    } else {
        crate::advice::AdviceSelector::Top(args.top.unwrap_or(5))
    }
}

fn advice_render(value: &Value, markdown: Option<&str>, format: AdviceFormat) -> Result<String> {
    match format {
        AdviceFormat::Json => Ok(serde_json::to_string_pretty(value)? + "\n"),
        AdviceFormat::Markdown => Ok(markdown
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| crate::advice::render_advice_markdown(value))),
    }
}

fn inference_argument(
    value: &Option<String>,
    flag: &str,
    pointer: &str,
) -> Result<String> {
    value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ClassifiedError::new(
                ErrorKind::Contract,
                "advisor_inference_configuration_required",
                format!("experimental inference requires an explicit {flag} value"),
            )
            .at(pointer)
            .into()
        })
}

fn immutable_model_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn inference_provider_config(
    args: &AdviseArgs,
    gate: &crate::advice::AdvisorReleaseGate,
) -> Result<(crate::advice::ProviderConfig, Option<crate::advice::ResourcePreflight>)> {
    let provider = args.provider.ok_or_else(|| {
        ClassifiedError::new(
            ErrorKind::Contract,
            "advisor_inference_configuration_required",
            "experimental inference requires an explicit --provider",
        )
        .at("/provider")
    })?;
    let model = inference_argument(&args.model, "--model", "/model")?;
    if model != gate.canonical_model {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "unsupported_advisor_model",
            format!(
                "Safeguard-only V1 requires model {}, received {model:?}",
                gate.canonical_model
            ),
        )
        .at("/model")
        .into());
    }
    let runtime_model =
        inference_argument(&args.runtime_model, "--runtime-model", "/runtime_model")?;
    let runtime_label =
        inference_argument(&args.runtime_label, "--runtime-label", "/runtime_label")?;
    if runtime_label.len() > 100 {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "advisor_runtime_label_invalid",
            "--runtime-label must be 100 characters or fewer",
        )
        .at("/runtime_label")
        .into());
    }
    let model_digest =
        inference_argument(&args.model_digest, "--model-digest", "/model_digest")?;
    if provider != crate::advice::ProviderKind::Mock && !immutable_model_digest(&model_digest) {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "advisor_model_digest_invalid",
            "--model-digest must be sha256:<64 hexadecimal characters>",
        )
        .at("/model_digest")
        .into());
    }

    let (endpoint, preflight, resource_guard) =
        if provider == crate::advice::ProviderKind::Mock {
            if args.mock_response.is_none() {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "provider_mock_missing",
                    "--provider mock requires --mock-response",
                )
                .at("/mock_response")
                .into());
            }
            (String::new(), None, None)
        } else {
            if !args.confirm_resources {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "advisor_resource_confirmation_required",
                    "experimental inference requires --confirm-resources after reviewing the model and host capacity values",
                )
                .at("/confirm_resources")
                .into());
            }
            let endpoint = inference_argument(&args.endpoint, "--endpoint", "/endpoint")?;
            let model_size_bytes = args.model_size_bytes.ok_or_else(|| {
                ClassifiedError::new(
                    ErrorKind::Contract,
                    "advisor_inference_configuration_required",
                    "experimental inference requires explicit --model-size-bytes",
                )
                .at("/model_size_bytes")
            })?;
            let estimated_peak_memory_bytes =
                args.estimated_peak_memory_bytes.ok_or_else(|| {
                    ClassifiedError::new(
                        ErrorKind::Contract,
                        "advisor_inference_configuration_required",
                        "experimental inference requires explicit --estimated-peak-memory-bytes",
                    )
                    .at("/estimated_peak_memory_bytes")
                })?;
            let (preflight, guard) = crate::advice::preflight_resources(
                gate,
                model_size_bytes,
                estimated_peak_memory_bytes,
            )?;
            eprintln!(
                "Advisor resource contract: model={} bytes; estimated peak={} bytes; required available={} bytes; host physical={} bytes; host available={} bytes; swap={} bytes; maximum swap growth={} bytes; consent=--confirm-resources",
                preflight.model_size_bytes,
                preflight.estimated_peak_memory_bytes,
                preflight.required_available_memory_bytes,
                preflight.system.physical_memory_bytes,
                preflight.system.available_memory_bytes,
                preflight.system.swap_used_bytes,
                preflight.maximum_swap_growth_bytes,
            );
            (endpoint, Some(preflight), Some(guard))
        };

    let mock_response = args
        .mock_response
        .as_deref()
        .map(|path| path.to_path_buf());
    Ok((
        crate::advice::ProviderConfig {
            kind: provider,
            endpoint,
            model,
            runtime_model,
            reasoning_effort: args.reasoning,
            connect_timeout: std::time::Duration::from_secs(args.connect_timeout_seconds),
            timeout: std::time::Duration::from_secs(args.timeout_seconds),
            max_response_bytes: args.max_response_bytes,
            max_output_tokens: args.max_output_tokens,
            context_window_tokens: args
                .runtime_context_tokens
                .unwrap_or_else(|| args.max_context_tokens.saturating_add(args.max_output_tokens)),
            runtime_label: Some(runtime_label),
            model_digest: Some(model_digest),
            mock_response,
            resource_guard,
            resource_preflight: preflight.clone(),
        },
        preflight,
    ))
}

fn run_advise(repo_root: &Path, args: AdviseArgs) -> Result<i32> {
    let command_started = std::time::Instant::now();
    let benchmark_mode = std::env::var("GIT_SLOP_ADVISOR_BENCHMARK").as_deref() == Ok("1");
    if args.evaluation_scenario != crate::advice::EvaluationScenario::Unmodified && !benchmark_mode
    {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "benchmark_scenario_disabled",
            "synthetic evaluation scenarios are reserved for the explicit advisor benchmark harness",
        )
        .at("/evaluation_scenario")
        .into());
    }
    let context_mode =
        args.context_only || (!args.infer && args.validate_artifact.is_none());
    let format = args.format.unwrap_or(if context_mode {
        AdviceFormat::Json
    } else {
        AdviceFormat::Markdown
    });
    if context_mode && format != AdviceFormat::Json {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "context_only_requires_json",
            "provider-independent context requires --format json; omit --format to select JSON automatically",
        )
        .at("/format")
        .into());
    }

    let gate = crate::advice::release_gate()?;
    let provider_config = if args.infer {
        let benchmark_inference_enabled = benchmark_mode
            && (cfg!(feature = "advisor-inference-benchmark")
                || args.provider == Some(crate::advice::ProviderKind::Mock));
        if !gate.public_inference_enabled && !benchmark_inference_enabled {
            return Err(ClassifiedError::new(
                ErrorKind::Contract,
                "advisor_inference_deferred",
                format!(
                    "model inference is disabled because the checked-in advisor release recommendation is {:?}; use provider-free context output or an advisor-inference-benchmark build on a separately controlled, adequately resourced host. Decision: {}",
                    gate.recommendation, gate.decision_record
                ),
            )
            .at("/infer")
            .into());
        }
        eprintln!("Advisor phase 1/4: validating release, capacity, and provider boundaries.");
        let (config, _preflight) = inference_provider_config(&args, &gate)?;
        crate::advice::probe(&config)?;
        Some(config)
    } else {
        None
    };

    let required_runtime_context_tokens = args
        .max_context_tokens
        .saturating_add(args.max_output_tokens);
    let runtime_context_tokens = args
        .runtime_context_tokens
        .unwrap_or(required_runtime_context_tokens);
    if runtime_context_tokens < required_runtime_context_tokens {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "runtime_context_too_small",
            format!(
                "--runtime-context-tokens must be at least {required_runtime_context_tokens} to hold the configured input and output token budgets"
            ),
        )
        .at("/runtime_context_tokens")
        .into());
    }
    let (report_value, report_path) =
        report_or_missing_with_currentness(repo_root, args.report.as_deref(), true)?;
    if let Some(artifact_path) = args.validate_artifact.as_deref() {
        let artifact_path = resolve_repo_path(repo_root, artifact_path);
        let artifact = crate::advice::load_and_validate_artifact(&artifact_path, &report_value)?;
        let rendered = advice_render(&artifact, None, format)?;
        if let Some(output) = args.output.as_deref() {
            write_generated_output(Some(&resolve_repo_path(repo_root, output)), rendered.as_bytes())?;
        } else {
            print_text(&rendered);
        }
        return Ok(0);
    }

    if args.infer {
        eprintln!("Advisor phase 2/4: building bounded deterministic context.");
    }
    let policies = crate::policy::resolve_for_advice(repo_root, &args.policies)?;
    let context_started = std::time::Instant::now();
    let input = crate::advice::build_input(
        &report_value,
        &report_path,
        repo_root,
        advice_selector(&args, repo_root),
        &policies,
        &crate::advice::BuildInputOptions {
            max_slices: args.max_slices,
            excerpt_bytes: args.excerpt_bytes,
            max_context_bytes: args.max_context_bytes,
            max_context_tokens: args.max_context_tokens,
            evaluation_scenario: args.evaluation_scenario,
        },
    )?;
    let context_elapsed_ms = context_started.elapsed().as_millis();
    if context_mode {
        let rendered = serde_json::to_string_pretty(&input)? + "\n";
        if let Some(output) = args.output.as_deref() {
            write_generated_output(Some(&resolve_repo_path(repo_root, output)), rendered.as_bytes())?;
        } else {
            print_text(&rendered);
        }
        let cache_path = (!args.ephemeral)
            .then(|| crate::advice::cache_input(repo_root, &input))
            .transpose()?;
        if let Some(cache_path) = cache_path {
            eprintln!(
                "Cached deterministic advice input at {}",
                cache_path.display()
            );
        }
        return Ok(0);
    }

    let mut provider_config = provider_config.expect("--infer resolves a provider configuration");
    provider_config.context_window_tokens = runtime_context_tokens;
    provider_config.mock_response = provider_config
        .mock_response
        .as_deref()
        .map(|path| resolve_repo_path(repo_root, path));
    eprintln!(
        "Advisor phase 3/4: invoking the explicit provider; press Ctrl-C to cancel. The timeout covers model loading and generation."
    );
    let provider = crate::advice::invoke(&input, &provider_config)?;
    eprintln!("Advisor phase 4/4: validating references and recomputing the verdict.");
    let validation_started = std::time::Instant::now();
    let validated = crate::advice::validate_response(&provider.response, &input, &policies)?;
    let validation_elapsed_ms = validation_started.elapsed().as_millis();
    let cache_path = (!args.ephemeral)
        .then(|| crate::advice::cache_input(repo_root, &input))
        .transpose()?;
    let run = crate::advice::AdviceRun::new(
        &input,
        &policies,
        &provider,
        &validated,
        crate::advice::AdviceTimings {
            context_elapsed_ms,
            provider_elapsed_ms: provider.elapsed_ms,
            validation_elapsed_ms,
            time_to_validated_artifact_ms: command_started.elapsed().as_millis(),
        },
    )?;
    let stored_paths = (!args.ephemeral)
        .then(|| crate::advice::write_artifacts(repo_root, &run))
        .transpose()?;
    let rendered = advice_render(&run.artifact, Some(&run.markdown), format)?;
    if let Some(output) = args.output.as_deref() {
        write_generated_output(Some(&resolve_repo_path(repo_root, output)), rendered.as_bytes())?;
    } else {
        print_text(&rendered);
    }
    if let Some((json_path, markdown_path)) = stored_paths {
        eprintln!(
            "Wrote validated advice artifacts: {} and {}",
            json_path.display(),
            markdown_path.display()
        );
    }
    if let Some(cache_path) = cache_path {
        eprintln!(
            "Cached deterministic advice input at {}",
            cache_path.display()
        );
    }
    Ok(0)
}
