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

fn emit_baseline_result(format: DisplayFormat, payload: &Value, text: &str) -> Result<()> {
    match format {
        DisplayFormat::Text => println!("{text}"),
        DisplayFormat::Json => print_text(&render_json(payload)?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(payload)?),
    }
    Ok(())
}

fn run_baseline(repo_root: &Path, args: BaselineArgs) -> Result<i32> {
    match args.command {
        BaselineCommand::Ensure {
            name,
            report,
            replace,
            allow_dirty,
            allow_incomplete_evidence,
            format,
        } => {
            let (loaded, source) = report_or_missing(repo_root, report.as_deref())?;
            let readiness = crate::report_ops::evaluate_report_readiness(
                &loaded,
                !allow_dirty,
                allow_incomplete_evidence,
            );
            if !readiness.comparison_ready {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_comparison_ready",
                    "Baseline source is not comparison-ready.",
                )
                .with_details(readiness.as_json())
                .into());
            }
            let path = baseline_path(repo_root, &name)?;
            let existing = load_report_at(&path)?;
            let requested_digest =
                hex::encode(sha2::Sha256::digest(serde_json::to_vec(&loaded)?));
            let status = match existing {
                Some(existing) if existing == loaded => "unchanged",
                Some(existing) if !replace => {
                    let stored_digest = hex::encode(sha2::Sha256::digest(serde_json::to_vec(&existing)?));
                    return Err(ClassifiedError::new(
                        ErrorKind::Contract,
                        "baseline_drift",
                        "Stored baseline differs from the requested report; pass --replace to update it explicitly.",
                    )
                    .at("/name")
                    .with_details(json!({"name": name, "stored_digest": stored_digest, "requested_digest": requested_digest, "flag": "--replace"}))
                    .into());
                }
                Some(_) => {
                    write_named_baseline(&path, &loaded, true)?;
                    "replaced"
                }
                None => {
                    write_named_baseline(&path, &loaded, false)?;
                    "created"
                }
            };
            emit_baseline_result(
                format,
                &json!({"schema_version":1,"command":"baseline ensure","name":name,"status":status,"report_digest":requested_digest,"source_report":source,"storage":"git_private","readiness":readiness.as_json()}),
                &format!("Baseline '{name}' is {status} for {}.", source.display()),
            )?;
            Ok(0)
        }
        BaselineCommand::Create {
            name,
            report,
            force,
            allow_dirty,
            allow_incomplete_evidence,
            format,
        } => {
            let (loaded, source) = report_or_missing(repo_root, report.as_deref())?;
            let readiness = crate::report_ops::evaluate_report_readiness(
                &loaded,
                !allow_dirty,
                allow_incomplete_evidence,
            );
            if !readiness.comparison_ready {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_comparison_ready",
                    "Baseline source is not comparison-ready.",
                )
                .with_details(readiness.as_json())
                .into());
            }
            let path = baseline_path(repo_root, &name)?;
            write_named_baseline(&path, &loaded, force)?;
            emit_baseline_result(
                format,
                &json!({"schema_version":1,"command":"baseline create","name":name,"source_report":source,"storage":"git_private","readiness":readiness.as_json()}),
                &format!("Created baseline '{name}' from {} in Git-private runtime storage.", source.display()),
            )?;
            Ok(0)
        }
        BaselineCommand::Update {
            name,
            report,
            allow_dirty,
            allow_incomplete_evidence,
            format,
        } => {
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
            let readiness = crate::report_ops::evaluate_report_readiness(
                &loaded,
                !allow_dirty,
                allow_incomplete_evidence,
            );
            if !readiness.comparison_ready {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_comparison_ready",
                    "Baseline source is not comparison-ready.",
                )
                .with_details(readiness.as_json())
                .into());
            }
            write_named_baseline(&path, &loaded, true)?;
            emit_baseline_result(
                format,
                &json!({"schema_version":1,"command":"baseline update","name":name,"source_report":source,"storage":"git_private","readiness":readiness.as_json()}),
                &format!("Updated baseline '{name}' from {}.", source.display()),
            )?;
            Ok(0)
        }
        BaselineCommand::List { format } => {
            let directory = config::git_runtime_dir(repo_root)?.join("baselines");
            let mut baselines = Vec::new();
            if directory.is_dir() {
                for entry in fs::read_dir(&directory)? {
                    let path = entry?.path();
                    if path.extension().and_then(|value| value.to_str()) != Some("json") {
                        continue;
                    }
                    let Some(report) = load_report_at(&path)? else { continue };
                    let readiness = crate::report_ops::evaluate_report_readiness(&report, true, false);
                    baselines.push(json!({
                        "name": path.file_stem().and_then(|value| value.to_str()).unwrap_or_default(),
                        "head_sha": report.pointer("/repo/head_sha"),
                        "generated_at": report.get("generated_at"),
                        "repository_id": report.pointer("/repo/repository_id"),
                        "scope": report.get("scope"),
                        "readiness": readiness.as_json(),
                    }));
                }
            }
            baselines.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
            let payload = json!({"schema_version": 1, "command": "baseline list", "storage": "git_private", "baselines": baselines});
            match format {
                DisplayFormat::Json => print_text(&render_json(&payload)?),
                DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
                DisplayFormat::Text => {
                    if baselines.is_empty() { println!("No named baselines."); }
                    for item in baselines {
                        println!(
                            "{} revision={} comparison_ready={}",
                            item["name"].as_str().unwrap_or_default(),
                            item["head_sha"].as_str().unwrap_or("unknown"),
                            item.pointer("/readiness/comparison_ready").and_then(Value::as_bool).unwrap_or(false)
                        );
                    }
                }
            }
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
                "readiness": crate::report_ops::evaluate_report_readiness(&report, true, false).as_json(),
                "storage_bytes": fs::metadata(&path).map(|value| value.len()).unwrap_or_default(),
                "updated_at": fs::metadata(&path).ok().and_then(|value| value.modified().ok()).map(|value| chrono::DateTime::<chrono::Utc>::from(value).to_rfc3339()),
                "report": {
                    "schema_version": report.get("schema_version"),
                    "generated_at": report.get("generated_at"),
                    "head_sha": report.pointer("/repo/head_sha"),
                    "scope": report.get("scope"),
                    "report_profile": report.pointer("/analyzer/report_profile"),
                    "evidence_completeness": report.get("evidence_completeness"),
                    "worktree_clean": report.pointer("/repo/worktree_clean"),
                    "analysis_status": report.pointer("/diagnostics/analysis/analysis_status"),
                    "collection_metadata": report.get("collection_metadata"),
                    "analysis_config_digest": report.pointer("/analyzer/analysis_config_digest"),
                    "evidence_config_digest": report.pointer("/analyzer/evidence_config_digest"),
                    "policy_config_digest": report.pointer("/analyzer/policy_config_digest"),
                    "presentation_config_digest": report.pointer("/analyzer/presentation_config_digest"),
                    "worktree_state_digest": report.pointer("/repo/worktree_state_digest"),
                    "analyzed_content_digest": report.pointer("/repo/analyzed_content_digest")
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
        BaselineCommand::Validate { name, format } => {
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
            emit_baseline_result(
                format,
                &json!({"schema_version":1,"command":"baseline validate","name":name,"valid":true}),
                &format!("Baseline '{name}' is valid."),
            )?;
            Ok(0)
        }
        BaselineCommand::Remove { name, format } => {
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
            emit_baseline_result(
                format,
                &json!({"schema_version":1,"command":"baseline remove","name":name,"removed":true}),
                &format!("Removed baseline '{name}'."),
            )?;
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
        return argument_error("/top", "--top", "--top must be greater than zero.", args.top);
    };
    if args.limit == 0 {
        return argument_error(
            "/limit",
            "--limit",
            "--limit must be greater than zero.",
            args.limit,
        );
    }
    let local_descriptor = |path: &Path| {
        if args.include_local_paths {
            path.to_string_lossy().into_owned()
        } else {
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("report.json")
                .to_string()
        }
    };
    let base_descriptor = if let Some(materialized) = materialized.as_ref() {
        format!(
            "{}@{}",
            base_report
                .pointer("/repo/repository_id")
                .and_then(Value::as_str)
                .unwrap_or("repository"),
            materialized.revision
        )
    } else if let Some(name) = args.baseline.as_deref() {
        format!("baseline:{name}")
    } else {
        local_descriptor(&base_path)
    };
    let payload = match compare_payload_with_policy(
        &base_report,
        &head_report,
        Some(&base_descriptor),
        Some(&local_descriptor(&args.head)),
        top,
        args.force,
        args.allow_incomplete_evidence,
        args.policy_from.as_str(),
    ) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    let output = match bounded_compare_output(
        &payload,
        args.detail,
        top,
        args.offset,
        args.limit,
        args.include_unchanged,
    ) {
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
