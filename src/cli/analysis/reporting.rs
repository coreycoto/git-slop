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
        DisplayFormat::Text if args.verbose => print_text(&render_explain_text(&payload)),
        DisplayFormat::Text => print_text(&render_explain_summary_text(&payload)),
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
