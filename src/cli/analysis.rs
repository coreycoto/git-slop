include!("analysis/estimate.rs");

fn run_find(repo_root: &Path, args: FindArgs) -> Result<i32> {
    if args.estimate_only {
        let config = config::load(repo_root)?;
        let normalized_scope = analyze::normalize_scope(args.scope.as_deref()).map_err(|error| {
            ClassifiedError::new(ErrorKind::Contract, "invalid_scope", format!("{error:#}"))
                .at("/scope")
                .with_details(json!({"scope": args.scope}))
        })?;
        if let Some(scope) = normalized_scope.as_deref() {
            if fs::symlink_metadata(repo_root.join(scope)).is_err() {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "scope_not_found",
                    format!("--scope does not exist in the repository: {scope}"),
                )
                .at("/scope")
                .with_details(json!({"scope": scope}))
                .into());
            }
        }
        let paths = git::list_tracked_files(repo_root)?
            .into_iter()
            .filter(|path| {
                normalized_scope
                    .as_deref()
                    .is_none_or(|scope| path == scope || path.starts_with(&format!("{scope}/")))
            })
            .collect::<Vec<_>>();
        if paths.is_empty() && !args.allow_empty_scope {
            return Err(ClassifiedError::new(
                ErrorKind::Contract,
                "empty_scope",
                "The selected estimate scope contains no tracked paths. Commit a tracked file, or pass `--allow-empty-scope` when an empty estimate is intentional.",
            )
            .at("/scope")
            .with_details(json!({"scope": normalized_scope}))
            .into());
        }
        let estimate = crate::estimate::build(repo_root, &paths, &config);
        let payload = json!({
            "schema_version": 1,
            "command": "find estimate",
            "scope": normalized_scope,
            "estimate": estimate
        });
        let format = args.format.unwrap_or_else(|| {
            if std::io::stdout().is_terminal() {
                DisplayFormat::Text
            } else {
                DisplayFormat::Json
            }
        });
        print_find_estimate(&payload, normalized_scope.as_deref(), format)?;
        return Ok(0);
    }
    let normalized_scope = analyze::normalize_scope(args.scope.as_deref()).map_err(|error| {
        ClassifiedError::new(ErrorKind::Contract, "invalid_scope", format!("{error:#}"))
            .at("/scope")
            .with_details(json!({"scope": args.scope}))
    })?;
    if let Some(scope) = normalized_scope.as_deref() {
        if fs::symlink_metadata(repo_root.join(scope)).is_err() {
            return Err(ClassifiedError::new(
                ErrorKind::Contract,
                "scope_not_found",
                format!("--scope does not exist in the repository: {scope}"),
            )
            .at("/scope")
            .with_details(json!({"scope": scope}))
            .into());
        }
        let selected = git::list_tracked_files(repo_root)?
            .into_iter()
            .any(|path| path == scope || path.starts_with(&format!("{scope}/")));
        if !selected && !args.allow_empty_scope {
            return Err(ClassifiedError::new(
                ErrorKind::Contract,
                "empty_scope",
                format!("--scope {scope:?} selected no tracked paths"),
            )
            .at("/scope")
            .with_details(json!({"scope": scope}))
            .into());
        }
    }
    let as_of = args
        .as_of
        .as_deref()
        .map(chrono::DateTime::parse_from_rfc3339)
        .transpose()
        .context("--as-of must be an RFC 3339 timestamp")?
        .map(|value| value.with_timezone(&chrono::Utc));
    let adoption = config::adoption_status(repo_root);
    let persistent_unadopted = !adoption.ready() && args.persist_unadopted;
    let auto_ephemeral = !adoption.ready()
        && !args.ephemeral
        && !args.persist_unadopted
        && args.state_dir.is_none()
        && args.output_dir.is_none();
    let ephemeral_root = (args.ephemeral || auto_ephemeral)
        .then(|| config::git_runtime_dir(repo_root).map(|path| path.join("ephemeral")))
        .transpose()?;
    let state_dir = ephemeral_root.clone().or(args.state_dir);
    let output_dir = ephemeral_root.or(args.output_dir);
    let result = analyze::run_find_with_options(
        repo_root,
        &analyze::FindOptions {
            allow_shallow: args.allow_shallow,
            scope: args.scope,
            progress: !args.quiet && !args.no_progress && std::io::stderr().is_terminal(),
            allow_empty_scope: args.allow_empty_scope,
            state_dir,
            output_dir,
            no_cache: args.no_cache || args.ephemeral,
            allow_degraded: args.allow_degraded,
            as_of,
            report_profile: args.report_profile.as_str().to_string(),
            compression: args.compression.as_str().to_string(),
        },
    )?;
    if args.quiet {
        return Ok(0);
    }
    if auto_ephemeral {
        println!(
            "Repository adoption is incomplete; this scan used Git-private ephemeral storage."
        );
        println!(
            "Next: run `git slop health`, `git slop doctor`, or `git slop html`; run `git slop init` when you want durable reports."
        );
    } else if persistent_unadopted {
        println!(
            "Persistent unadopted output was explicitly enabled; run `git slop init` to keep runtime artifacts ignored."
        );
    }
    print_text(&result.terminal);
    print_scan_receipt(&result);
    println!("Wrote report to {}.", result.report_json.display());
    if result.report_yaml.exists() {
        println!("Wrote YAML report to {}.", result.report_yaml.display());
    }
    println!("Wrote summary to {}.", result.summary_md.display());
    println!(
        "Wrote repository health summary to {}.",
        result.health_md.display()
    );
    if let Some(path) = result.compressed_report {
        println!("Wrote compressed report to {}.", path.display());
    }
    Ok(0)
}

fn run_show(repo_root: &Path, args: ShowArgs) -> Result<i32> {
    let (loaded, report_path) = report_or_missing_with_currentness(
        repo_root,
        args.report.as_deref(),
        args.require_current,
    )?;
    let target = selector_path(repo_root, &args.target_path);
    let Some(payload) = show_payload(&loaded, &target) else {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "selector_not_found",
            format!(
                "No record found for '{}' in {}.",
                args.target_path,
                report_path.display()
            ),
        )
        .at("/target_path")
        .with_details(json!({"selector": args.target_path, "report": report_path}))
        .into());
    };
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_show_text(&payload)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn explain_selector(args: &ExplainArgs, repo_root: &Path) -> Result<ExplainSelector> {
    if let Some(path) = &args.path {
        Ok(ExplainSelector::Path(selector_path(repo_root, path)))
    } else if let Some(id) = &args.cluster {
        Ok(ExplainSelector::Cluster(id.clone()))
    } else if let Some(id) = &args.relationship {
        Ok(ExplainSelector::Relationship(id.clone()))
    } else {
        let count = args.top.unwrap_or(5);
        match usize::try_from(count).ok().filter(|count| *count > 0) {
            Some(count) => Ok(ExplainSelector::Top(count)),
            None => Err(ClassifiedError::new(
                ErrorKind::Contract,
                "invalid_argument",
                "--top must be greater than zero",
            )
            .at("/top")
            .into()),
        }
    }
}

fn run_explain(repo_root: &Path, args: ExplainArgs) -> Result<i32> {
    if args.include_repository_context && !(256..=4096).contains(&args.excerpt_bytes) {
        return argument_error(
            "/excerpt_bytes",
            "--excerpt-bytes",
            "--excerpt-bytes must be between 256 and 4096",
            args.excerpt_bytes,
        );
    }
    let (loaded, report_path) = report_or_missing_with_currentness(
        repo_root,
        args.report.as_deref(),
        args.require_current,
    )?;
    let selector = explain_selector(&args, repo_root)?;
    let payload = match explain_payload(&loaded, Some(selector)) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    if let Some(output_dir) = args.prompt_pack.as_deref() {
        ensure_prompt_pack_target(output_dir)?;
        write_prompt_pack(
            "explain",
            &payload,
            &loaded,
            &report_path,
            output_dir,
            PromptPackOptions {
                repository_root: args.include_repository_context.then_some(repo_root),
                excerpt_bytes: args.excerpt_bytes,
                force: args.force,
                include_local_paths: args.include_local_paths,
            },
        )?;
    }
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_explain_text(&payload)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn plan_selector(args: &PlanArgs, repo_root: &Path) -> PlanSelector {
    if let Some(path) = &args.path {
        PlanSelector::Path(selector_path(repo_root, path))
    } else if let Some(id) = &args.cluster {
        PlanSelector::Cluster(id.clone())
    } else {
        PlanSelector::Relationship(args.relationship.clone().unwrap_or_default())
    }
}

fn run_plan(repo_root: &Path, args: PlanArgs) -> Result<i32> {
    if args.include_repository_context && !(256..=4096).contains(&args.excerpt_bytes) {
        return argument_error(
            "/excerpt_bytes",
            "--excerpt-bytes",
            "--excerpt-bytes must be between 256 and 4096",
            args.excerpt_bytes,
        );
    }
    let (loaded, report_path) = report_or_missing_with_currentness(
        repo_root,
        args.report.as_deref(),
        args.require_current,
    )?;
    let Some(max_slices) = usize::try_from(args.max_slices)
        .ok()
        .filter(|count| *count > 0)
    else {
        return argument_error(
            "/max_slices",
            "--max-slices",
            "--max-slices must be greater than zero",
            args.max_slices,
        );
    };
    let mut payload = match plan_payload(&loaded, plan_selector(&args, repo_root), max_slices) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    let canonical_report = serde_json::to_vec(&loaded)?;
    let report_digest = hex::encode(sha2::Sha256::digest(&canonical_report));
    let baseline_name = format!("plan-{}", &report_digest[..12]);
    let presentation_root = if repo_root.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_default()
    } else {
        repo_root.to_path_buf()
    };
    let canonical_repo_root = presentation_root
        .canonicalize()
        .unwrap_or(presentation_root);
    let canonical_report_path = report_path
        .canonicalize()
        .unwrap_or_else(|_| report_path.clone());
    let repo_relative_report = canonical_report_path
        .strip_prefix(&canonical_repo_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"));
    let report_command_path = repo_relative_report.clone().unwrap_or_else(|| {
        if args.include_local_paths {
            canonical_report_path.to_string_lossy().replace('\\', "/")
        } else {
            "<SOURCE_REPORT>".to_string()
        }
    });
    let quoted_report = format!("'{}'", report_command_path.replace('\'', "'\\''"));
    payload["source_report"] = json!({
        "path": repo_relative_report.or_else(|| args.include_local_paths.then_some(report_command_path.clone())),
        "descriptor": if canonical_report_path.starts_with(&canonical_repo_root) { "repo_relative" } else if args.include_local_paths { "local_path" } else { "logical_source_report" },
        "sha256": report_digest,
        "baseline_name": baseline_name,
    });
    for slice in payload["proposed_slices"].as_array_mut().into_iter().flatten() {
        slice["baseline_command"] = json!(format!(
            "git slop baseline ensure --name {baseline_name} --report {quoted_report}"
        ));
        slice["baseline_update_command"] = json!(format!(
            "git slop baseline ensure --name {baseline_name} --report {quoted_report} --replace"
        ));
        slice["rerun_command"] = json!(format!(
            "git slop find && git slop compare --baseline {baseline_name} --head .slop/latest/report.json --detail summary --fail-on-regression"
        ));
    }
    if let Some(output_dir) = args.prompt_pack.as_deref() {
        ensure_prompt_pack_target(output_dir)?;
        write_prompt_pack(
            "plan",
            &payload,
            &loaded,
            &report_path,
            output_dir,
            PromptPackOptions {
                repository_root: args.include_repository_context.then_some(repo_root),
                excerpt_bytes: args.excerpt_bytes,
                force: args.force,
                include_local_paths: args.include_local_paths,
            },
        )?;
    }
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_plan_text(&payload)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}
