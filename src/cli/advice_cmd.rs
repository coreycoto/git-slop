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

fn provider_free_context_markdown(input: &Value) -> String {
    let candidates = input["candidates"].as_array().map_or(&[][..], Vec::as_slice);
    let implementable = candidates
        .iter()
        .filter(|candidate| candidate["disposition"] == "implementable")
        .count();
    let investigate = candidates.len().saturating_sub(implementable);
    let policy_count = input
        .pointer("/policies/rules")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let excerpt_count = input["repository_excerpts"]
        .as_array()
        .map_or(0, Vec::len);
    let missing_count = input["missing_evidence"].as_array().map_or(0, Vec::len);
    let truncated = input
        .pointer("/limits/truncation/occurred")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let selector_kind = input
        .pointer("/selector/kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let selector_value = input
        .pointer("/selector/value")
        .map(|value| match value {
            Value::String(value) => value.clone(),
            _ => value.to_string(),
        })
        .unwrap_or_else(|| "unknown".to_string());
    let digest = input
        .get("context_digest")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let tokens = input
        .pointer("/limits/estimated_context_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let mut output = format!(
        "# Git Slop provider-free advice context\n\n\
         Status: **ready for human review; no model or provider was configured or contacted**\n\n\
         - Selector: `{selector_kind}` = `{}`\n\
         - Candidates: {} ({implementable} implementable, {investigate} investigate)\n\
         - Applicable policy rules: {policy_count}\n\
         - Repository excerpts: {excerpt_count}\n\
         - Missing evidence: {missing_count}\n\
         - Context truncated: {}\n\
         - Estimated tokens: {tokens}\n\
         - Context digest: `{digest}`\n\n\
         ## Candidates\n\n",
        crate::text::visible_controls(&selector_value),
        candidates.len(),
        if truncated { "yes" } else { "no" },
    );
    for candidate in candidates {
        let disposition = candidate["disposition"].as_str().unwrap_or("investigate");
        let title = candidate
            .pointer("/interpretation/title")
            .and_then(Value::as_str)
            .unwrap_or("Untitled candidate");
        let objective = candidate
            .pointer("/interpretation/objective")
            .and_then(Value::as_str)
            .unwrap_or("Review the supplied evidence.");
        output.push_str(&format!(
            "- **{}** — {}\n  - {}\n",
            disposition,
            crate::text::visible_controls(title),
            crate::text::visible_controls(objective),
        ));
    }
    output.push_str(
        "\n## Next step\n\nRerun with `--context-only --format json` to inspect or export the complete byte-stable machine context. Provider-free context is advisory and does not mutate source.\n",
    );
    output
}

fn context_render(input: &Value, format: AdviceFormat) -> Result<String> {
    match format {
        AdviceFormat::Json => Ok(serde_json::to_string_pretty(input)? + "\n"),
        AdviceFormat::Markdown => Ok(provider_free_context_markdown(input)),
    }
}

fn classify_advisor_error(
    error: anyhow::Error,
    fallback_code: &'static str,
    fallback_kind: ErrorKind,
) -> anyhow::Error {
    if error.downcast_ref::<ClassifiedError>().is_some() {
        return error;
    }
    let diagnostic = format!("{error:#}");
    let code = [
        "provider_endpoint_unsupported",
        "provider_endpoint_invalid",
        "provider_remote_unsupported",
        "provider_response_too_large",
        "provider_response_invalid",
        "provider_http_invalid",
        "provider_http_unsupported",
        "provider_timeout",
        "provider_unavailable",
        "provider_http_error",
        "provider_model_identity_missing",
        "provider_model_mismatch",
        "provider_completion_state_missing",
        "provider_incomplete_response",
        "provider_resource_guard_unavailable",
        "provider_resource_guard_triggered",
        "advisor_input_too_large",
    ]
    .into_iter()
    .find(|code| diagnostic.contains(code))
    .unwrap_or(fallback_code);
    let kind = if code.contains("resource_guard")
        || matches!(code, "provider_response_too_large" | "advisor_input_too_large")
    {
        ErrorKind::ResourceLimit
    } else {
        fallback_kind
    };
    ClassifiedError::new(kind, code, diagnostic)
        .at("/advisor")
        .into()
}

fn classify_advisor_runtime_error(error: anyhow::Error) -> anyhow::Error {
    classify_advisor_error(error, "provider_operation_failed", ErrorKind::Repository)
}

fn classify_advisor_validation_error(error: anyhow::Error) -> anyhow::Error {
    classify_advisor_error(error, "provider_response_invalid", ErrorKind::Contract)
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

fn privacy_safe_runtime_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && !value.ends_with(':')
        && !value.contains(":/")
        && !value.contains("::")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+:/@-".contains(&byte))
}

fn privacy_safe_runtime_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte == b' '
                || b"._+-()".contains(&byte)
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
    if !privacy_safe_runtime_identifier(&runtime_model) {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "advisor_runtime_model_invalid",
            "--runtime-model must be a privacy-safe runtime identifier of 200 characters or fewer",
        )
        .at("/runtime_model")
        .into());
    }
    let runtime_label =
        inference_argument(&args.runtime_label, "--runtime-label", "/runtime_label")?;
    if !privacy_safe_runtime_label(&runtime_label) {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "advisor_runtime_label_invalid",
            "--runtime-label must be a short privacy-safe runtime name and version",
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
                "Advisor resource contract: model={} bytes; estimated peak={} bytes; required available={} bytes; host physical={} bytes; host available={} bytes; swap={} bytes; maximum initial swap={} bytes; maximum swap growth={} bytes; consent=--confirm-resources",
                preflight.model_size_bytes,
                preflight.estimated_peak_memory_bytes,
                preflight.required_available_memory_bytes,
                preflight.system.physical_memory_bytes,
                preflight.system.available_memory_bytes,
                preflight.system.swap_used_bytes,
                preflight.maximum_initial_swap_used_bytes,
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
    let format = args.format.unwrap_or(if context_mode && args.context_only {
        AdviceFormat::Json
    } else {
        AdviceFormat::Markdown
    });

    let gate = if args.infer {
        let gate = crate::advice::release_gate()?;
        let benchmark_inference_enabled = benchmark_mode
            && (cfg!(feature = "advisor-inference-benchmark")
                || args.provider == Some(crate::advice::ProviderKind::Mock));
        if !gate.public_inference_enabled && !benchmark_inference_enabled {
            return Err(ClassifiedError::new(
                ErrorKind::Contract,
                "advisor_inference_deferred",
                format!(
                    "model inference is unavailable in public releases because the checked-in advisor recommendation is {:?}. Use provider-free context output; maintainer inference research is restricted to a separately provisioned, adequately resourced host. Decision: {}",
                    gate.recommendation, gate.decision_record
                ),
            )
            .at("/infer")
            .into());
        }
        Some(gate)
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
    if args.infer {
        eprintln!(
            "Advisor phase 1/4: validating the report, policy, context, and release boundaries without provider contact."
        );
    }
    let (report_value, report_path) =
        report_or_missing_with_currentness(repo_root, args.report.as_deref(), true)?;
    if let Some(artifact_path) = args.validate_artifact.as_deref() {
        let artifact_path = resolve_repo_path(repo_root, artifact_path);
        let artifact = crate::advice::load_and_validate_artifact(&artifact_path, &report_value)
            .map_err(|error| {
                classify_advisor_error(error, "advisor_artifact_invalid", ErrorKind::Contract)
            })?;
        let rendered = advice_render(&artifact, None, format)?;
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
    if context_mode {
        let rendered = context_render(&input, format)?;
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

    eprintln!(
        "Advisor phase 2/4: validating capacity and probing the explicit provider only after local validation completed."
    );
    let (mut provider_config, _preflight) = inference_provider_config(
        &args,
        gate.as_ref()
            .expect("non-context advice requires an inference release gate"),
    )?;
    provider_config.context_window_tokens = runtime_context_tokens;
    provider_config.mock_response = provider_config
        .mock_response
        .as_deref()
        .map(|path| resolve_repo_path(repo_root, path));
    crate::advice::probe(&provider_config).map_err(classify_advisor_runtime_error)?;
    eprintln!(
        "Advisor phase 3/4: invoking the explicit provider; press Ctrl-C to cancel. The timeout covers model loading and generation."
    );
    let provider = crate::advice::invoke(&input, &provider_config)
        .map_err(classify_advisor_runtime_error)?;
    eprintln!("Advisor phase 4/4: validating references and recomputing the verdict.");
    let validation_started = std::time::Instant::now();
    let validated = crate::advice::validate_response(&provider.response, &input, &policies)
        .map_err(classify_advisor_validation_error)?;
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

#[cfg(test)]
mod advice_cmd_tests {
    use super::*;

    #[test]
    fn runtime_provenance_identifiers_are_privacy_safe() {
        assert!(privacy_safe_runtime_identifier("gpt-oss-safeguard:20b"));
        assert!(privacy_safe_runtime_identifier("org/model@sha256:abc"));
        assert!(!privacy_safe_runtime_identifier("/Users/example/model"));
        assert!(!privacy_safe_runtime_identifier("C:/Users/example/model"));
        assert!(!privacy_safe_runtime_identifier("private path/model"));
        assert!(!privacy_safe_runtime_identifier("model\nsecret"));
        assert!(privacy_safe_runtime_label("Runtime 1.2 (dedicated)"));
        assert!(!privacy_safe_runtime_label(" Runtime 1.2"));
        assert!(!privacy_safe_runtime_label("Runtime /Users/example"));
    }
}
