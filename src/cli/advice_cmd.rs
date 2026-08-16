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

fn run_advise(repo_root: &Path, args: AdviseArgs) -> Result<i32> {
    let command_started = std::time::Instant::now();
    if args.evaluation_scenario != crate::advice::EvaluationScenario::Unmodified
        && std::env::var("GIT_SLOP_ADVISOR_BENCHMARK").as_deref() != Ok("1")
    {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "benchmark_scenario_disabled",
            "synthetic evaluation scenarios are reserved for the explicit advisor benchmark harness",
        )
        .at("/evaluation_scenario")
        .into());
    }
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
        let rendered = advice_render(&artifact, None, args.format)?;
        if let Some(output) = args.output.as_deref() {
            write_generated_output(Some(&resolve_repo_path(repo_root, output)), rendered.as_bytes())?;
        } else {
            print_text(&rendered);
        }
        return Ok(0);
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
    let cache_path = (!args.ephemeral)
        .then(|| crate::advice::cache_input(repo_root, &input))
        .transpose()?;
    if args.context_only {
        if !matches!(args.format, AdviceFormat::Json) {
            return Err(ClassifiedError::new(
                ErrorKind::Contract,
                "context_only_requires_json",
                "--context-only requires --format json so the exact provider-independent input is preserved",
            )
            .at("/format")
            .into());
        }
        let rendered = serde_json::to_string_pretty(&input)? + "\n";
        if let Some(output) = args.output.as_deref() {
            write_generated_output(Some(&resolve_repo_path(repo_root, output)), rendered.as_bytes())?;
        } else {
            print_text(&rendered);
        }
        if let Some(cache_path) = cache_path {
            eprintln!(
                "Cached deterministic advice input at {}",
                cache_path.display()
            );
        }
        return Ok(0);
    }

    let model = args
        .model
        .or_else(|| std::env::var("GIT_SLOP_ADVISOR_MODEL").ok())
        .unwrap_or_else(|| "openai/gpt-oss-safeguard-20b".to_string());
    if model != "openai/gpt-oss-safeguard-20b" {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "unsupported_advisor_model",
            format!(
                "Safeguard-only V1 requires model openai/gpt-oss-safeguard-20b, received {model:?}"
            ),
        )
        .at("/model")
        .into());
    }
    let runtime_model = args
        .runtime_model
        .or_else(|| std::env::var("GIT_SLOP_ADVISOR_RUNTIME_MODEL").ok())
        .unwrap_or_else(|| model.clone());
    let endpoint = args
        .endpoint
        .or_else(|| std::env::var("GIT_SLOP_ADVISOR_ENDPOINT").ok())
        .unwrap_or_else(|| match args.provider {
            crate::advice::ProviderKind::Ollama => {
                "http://127.0.0.1:11434/api/chat".to_string()
            }
            _ => "http://127.0.0.1:11434/v1/chat/completions".to_string(),
        });
    let mock_response = args
        .mock_response
        .as_deref()
        .map(|path| resolve_repo_path(repo_root, path));
    let provider = crate::advice::invoke(
        &input,
        &crate::advice::ProviderConfig {
            kind: args.provider,
            endpoint,
            model,
            runtime_model,
            reasoning_effort: args.reasoning,
            allow_remote: args.allow_remote,
            timeout: std::time::Duration::from_secs(args.timeout_seconds),
            max_response_bytes: args.max_response_bytes,
            max_output_tokens: args.max_output_tokens,
            context_window_tokens: runtime_context_tokens,
            runtime_label: args.runtime_label,
            model_digest: args.model_digest,
            mock_response,
        },
    )?;
    let validation_started = std::time::Instant::now();
    let validated = crate::advice::validate_response(&provider.response, &input, &policies)?;
    let validation_elapsed_ms = validation_started.elapsed().as_millis();
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
    let rendered = advice_render(&run.artifact, Some(&run.markdown), args.format)?;
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
    Ok(0)
}
