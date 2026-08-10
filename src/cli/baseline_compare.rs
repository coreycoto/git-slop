fn baseline_path(repo_root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_baseline_name",
            "Baseline names must be 1-64 ASCII letters, digits, dots, dashes, or underscores.",
        )
        .at("/name")
        .with_details(json!({"name": name}))
        .into());
    }
    Ok(config::git_runtime_dir(repo_root)?
        .join("baselines")
        .join(format!("{name}.json")))
}

fn write_named_baseline(path: &Path, report: &Value, replace: bool) -> Result<()> {
    if path.exists() && !replace {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "baseline_exists",
            format!("Baseline already exists: {}", path.display()),
        )
        .at("/name")
        .with_details(json!({"path": path}))
        .into());
    }
    let parent = path.parent().expect("baseline path has parent");
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".baseline-{}-tmp", std::process::id()));
    fs::write(&temporary, render_json(report)?)?;
    let backup = parent.join(format!(".baseline-{}-backup", std::process::id()));
    let had_existing = path.exists();
    if had_existing {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        return Err(error.into());
    }
    if had_existing {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn run_baseline(repo_root: &Path, args: BaselineArgs) -> Result<i32> {
    match args.command {
        BaselineCommand::Create {
            name,
            report,
            force,
        } => {
            let (loaded, source) = report_or_missing(repo_root, report.as_deref())?;
            let path = baseline_path(repo_root, &name)?;
            write_named_baseline(&path, &loaded, force)?;
            println!(
                "Created baseline '{name}' from {} in Git-private runtime storage.",
                source.display()
            );
            Ok(0)
        }
        BaselineCommand::Update { name, report } => {
            let path = baseline_path(repo_root, &name)?;
            if !path.exists() {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            }
            let (loaded, source) = report_or_missing(repo_root, report.as_deref())?;
            write_named_baseline(&path, &loaded, true)?;
            println!("Updated baseline '{name}' from {}.", source.display());
            Ok(0)
        }
        BaselineCommand::Inspect { name, format } => {
            let path = baseline_path(repo_root, &name)?;
            let Some(report) = load_report_at(&path)? else {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            };
            let payload = json!({
                "schema_version": 1,
                "command": "baseline inspect",
                "name": name,
                "storage": "git_private",
                "report": {
                    "schema_version": report.get("schema_version"),
                    "generated_at": report.get("generated_at"),
                    "head_sha": report.pointer("/repo/head_sha"),
                    "scope": report.get("scope"),
                    "report_profile": report.pointer("/analyzer/report_profile"),
                    "evidence_completeness": report.get("evidence_completeness")
                }
            });
            match format {
                DisplayFormat::Json => print_text(&render_json(&payload)?),
                DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
                DisplayFormat::Text => println!(
                    "baseline={} revision={} generated_at={} storage=git_private",
                    payload["name"].as_str().unwrap_or_default(),
                    payload
                        .pointer("/report/head_sha")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown"),
                    payload
                        .pointer("/report/generated_at")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                ),
            }
            Ok(0)
        }
        BaselineCommand::Validate { name } => {
            let path = baseline_path(repo_root, &name)?;
            let Some(_) = load_report_at(&path)? else {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            };
            println!("Baseline '{name}' is valid.");
            Ok(0)
        }
        BaselineCommand::Remove { name } => {
            let path = baseline_path(repo_root, &name)?;
            if !path.exists() {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            }
            fs::remove_file(path)?;
            println!("Removed baseline '{name}'.");
            Ok(0)
        }
    }
}

fn run_compare(repo_root: &Path, args: CompareArgs) -> Result<i32> {
    let head_report = report_or_missing(Path::new(""), Some(&args.head))?.0;
    let inferred_scope = args.scope.clone().or_else(|| {
        head_report
            .pointer("/scope/path")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    });
    let analysis_clock = head_report
        .pointer("/analyzer/analysis_clock")
        .or_else(|| head_report.get("generated_at"))
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&chrono::Utc));
    let materialized = if let Some(reference) = args.base_ref.as_deref() {
        Some(crate::baseline::MaterializedBaseline::create(
            repo_root,
            reference,
            inferred_scope,
            args.allow_shallow,
            analysis_clock,
        )?)
    } else {
        None
    };
    let named_baseline_path = args
        .baseline
        .as_deref()
        .map(|name| baseline_path(repo_root, name))
        .transpose()?;
    let base_path = args
        .base
        .as_deref()
        .map(Path::to_path_buf)
        .or_else(|| materialized.as_ref().map(|value| value.report_path.clone()))
        .or(named_baseline_path)
        .expect("Clap requires --base, --base-ref, or --baseline");
    let base_report = report_or_missing(Path::new(""), Some(&base_path))?.0;
    let Some(top) = usize::try_from(args.top).ok().filter(|count| *count > 0) else {
        return usage_error("--top must be greater than zero.");
    };
    let base_descriptor = if let Some(materialized) = materialized.as_ref() {
        format!(
            "git:{}@{}",
            args.base_ref.as_deref().unwrap_or("unknown"),
            materialized.revision
        )
    } else if let Some(name) = args.baseline.as_deref() {
        format!("baseline:{name}")
    } else {
        base_path.to_string_lossy().into_owned()
    };
    let payload = match compare_payload_with_policy(
        &base_report,
        &head_report,
        Some(&base_descriptor),
        Some(&args.head.to_string_lossy()),
        top,
        args.force,
        args.allow_incomplete_evidence,
        args.policy_from.as_str(),
    ) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    let output = match bounded_compare_output(&payload, args.detail, top, args.offset, args.limit) {
        Ok(output) => output,
        Err(error) => return usage_error(error),
    };
    let mut output = output;
    if let Some(materialized) = materialized.as_ref() {
        output["baseline_materialization"] = json!({
            "reference": args.base_ref,
            "resolved_revision": materialized.revision,
            "isolated_worktree": true,
            "copied_head_config": materialized.copied_head_config,
            "cache_disabled": true
        });
    }
    match args.format {
        CompareFormat::Json => print_text(&render_json(&output)?),
        CompareFormat::Text => print_text(&render_compare_text(&output, top)),
        CompareFormat::Yaml => print_text(&serde_yaml::to_string(&output)?),
        CompareFormat::Ndjson => print_text(&render_compare_ndjson(&output)?),
    }
    let regressions = payload
        .pointer("/summary/regression_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    Ok(if args.fail_on_regression && regressions > 0 {
        1
    } else {
        0
    })
}
