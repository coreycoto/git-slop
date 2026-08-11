fn run_report(args: ReportArgs) -> Result<i32> {
    match args.command {
        ReportCommand::Validate { path, allow_legacy } => {
            match report::load_report_with_legacy(&path, allow_legacy) {
                Ok(value) => {
                    println!(
                        "Report is valid: {} (schema {}).",
                        path.display(),
                        value["schema_version"]
                    );
                    Ok(0)
                }
                Err(error) => {
                    let violations = fs::read_to_string(&path)
                        .ok()
                        .and_then(|source| serde_json::from_str::<Value>(&source).ok())
                        .map(|report| report::validation_violations(&report))
                        .unwrap_or_default();
                    Err(ClassifiedError::new(
                        ErrorKind::Contract,
                        "report_invalid",
                        format!("{error:#}"),
                    )
                    .at("/report")
                    .with_details(json!({"path": path, "violations": violations}))
                    .into())
                }
            }
        }
        ReportCommand::Migrate { path, output } => {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let value: Value = serde_json::from_str(&source)
                .with_context(|| format!("invalid git-slop report JSON: {}", path.display()))?;
            let migrated = report::migrate_legacy_report(value)?;
            report::write_json_atomically(&output, &migrated)?;
            println!(
                "Migrated {} to schema 5 at {}.",
                path.display(),
                output.display()
            );
            Ok(0)
        }
        ReportCommand::Schema => {
            print_text(&render_json(&report::schema())?);
            Ok(0)
        }
    }
}

fn run_sarif(repo_root: &Path, args: SarifArgs) -> Result<i32> {
    let (loaded, report_path) = report_or_missing(repo_root, args.report.as_deref())?;
    let top = match args.top {
        None => None,
        Some(value) => match usize::try_from(value).ok().filter(|count| *count > 0) {
            Some(value) => Some(value),
            None => return argument_error("/top", "--top", "--top must be greater than zero.", value),
        },
    };
    let report_descriptor = args
        .include_local_paths
        .then(|| report_path.to_string_lossy().to_string());
    let payload = match sarif_payload(&loaded, report_descriptor.as_deref(), top) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    let rendered = render_json(&payload)?;
    if let Some(output) = args.output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&output, rendered)
            .with_context(|| format!("failed to write {}", output.display()))?;
        println!("Wrote SARIF report to {}.", output.display());
    } else {
        print_text(&rendered);
    }
    Ok(0)
}

fn run_health(repo_root: &Path, args: HealthArgs) -> Result<i32> {
    let (mut loaded, _) = report_or_missing(repo_root, args.report.as_deref())?;
    let rollup = match health::health_rollup_from_report(&loaded) {
        Ok(rollup) => rollup,
        Err(error) => return usage_error(error),
    };
    let mut health_value = serde_json::to_value(rollup)?;
    if let (Some(existing), Some(derived)) = (
        loaded.get("health").and_then(Value::as_object),
        health_value.as_object_mut(),
    ) {
        for (key, value) in existing {
            derived.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    if let Some(object) = loaded.as_object_mut() {
        object.insert("health".to_string(), health_value);
    }
    match args.format {
        HealthFormat::Text => print_text(&report::render_terminal(&loaded)),
        HealthFormat::Markdown => {
            let rendered = match health::render_health_from_report(&loaded) {
                Ok(rendered) => rendered,
                Err(error) => return usage_error(error),
            };
            print_text(&rendered);
        }
        HealthFormat::Github => {
            print_text(&render_github_annotations(&loaded, args.max_annotations));
        }
        HealthFormat::Json => {
            print_text(&render_json(&health_json_payload(&loaded))?);
        }
    }
    Ok(0)
}

fn diff_values(current: &Value, defaults: &Value) -> Value {
    match (current, defaults) {
        (Value::Object(current), Value::Object(defaults)) => {
            let mut result = serde_json::Map::new();
            for (key, value) in current {
                if matches!(key.as_str(), "tokenizer" | "context_bands") {
                    continue;
                }
                let difference = defaults
                    .get(key)
                    .map_or_else(|| value.clone(), |default| diff_values(value, default));
                if !difference.is_null()
                    && !difference
                        .as_object()
                        .is_some_and(serde_json::Map::is_empty)
                {
                    result.insert(key.clone(), difference);
                }
            }
            Value::Object(result)
        }
        _ if current == defaults => Value::Null,
        _ => current.clone(),
    }
}

fn load_config_contract(repo_root: &Path) -> Result<Value> {
    config::load(repo_root).map_err(|error| {
        let message = format!("{error:#}");
        let pointer = message
            .split_whitespace()
            .map(|token| token.trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '.' && character != '_' && character != '[' && character != ']'))
            .find(|token| token.contains('.') && !token.ends_with(".yaml"))
            .map(|token| format!("/{}", token.replace('.', "/")))
            .unwrap_or_else(|| "/config".to_string());
        ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_configuration",
            message,
        )
        .at(pointer)
        .with_details(json!({"config_path": config::config_path(repo_root)}))
        .into()
    })
}

fn run_config(repo_root: &Path, args: ConfigArgs) -> Result<i32> {
    match args.command {
        ConfigCommand::Show { effective } => {
            if effective {
                print_text(&serde_yaml::to_string(&load_config_contract(repo_root)?)?);
            } else {
                let path = config::config_path(repo_root);
                if path.exists() {
                    print_text(&fs::read_to_string(path)?);
                } else {
                    print_text(config::MINIMAL_CONFIG);
                }
            }
        }
        ConfigCommand::Validate => {
            load_config_contract(repo_root)?;
            let path = config::config_path(repo_root);
            if path.exists() {
                println!("Configuration is valid: {}", path.display());
            } else {
                println!(
                    "Configuration is valid: built-in defaults ({} is absent).",
                    path.display()
                );
            }
        }
        ConfigCommand::DiffDefaults => {
            let diff = diff_values(&load_config_contract(repo_root)?, &config::default_config());
            print_text(&serde_yaml::to_string(&diff)?);
        }
        ConfigCommand::Migrate => {
            let effective = load_config_contract(repo_root)?;
            let mut diff = diff_values(&effective, &config::default_config());
            if let Some(object) = diff.as_object_mut() {
                object.insert("schema_version".into(), json!(2));
            }
            config::ensure_state_dirs(repo_root)?;
            fs::write(
                config::config_path(repo_root),
                serde_yaml::to_string(&diff)?,
            )?;
            println!(
                "Migrated {} to schema 2.",
                config::config_path(repo_root).display()
            );
        }
        ConfigCommand::Schema => print_text(&render_json(&config::schema())?),
    }
    Ok(0)
}

fn run_doctor(repo_root: &Path, args: DoctorArgs) -> Result<i32> {
    let repo = git::repo_metadata(repo_root)?;
    let config_result = config::load(repo_root);
    let config_exists = config::config_path(repo_root).is_file();
    let report_path = default_report_path(repo_root);
    let report_status = if report_path.exists() {
        match report::load_report(&report_path) {
            Ok(_) => "compatible",
            Err(_) => "invalid",
        }
    } else {
        "missing"
    };
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
    let tracked_paths = git::list_tracked_files(repo_root)?
        .into_iter()
        .filter(|path| {
            normalized_scope
                .as_deref()
                .is_none_or(|scope| path == scope || path.starts_with(&format!("{scope}/")))
        })
        .collect::<Vec<_>>();
    if tracked_paths.is_empty() && normalized_scope.is_some() {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "empty_scope",
            normalized_scope.as_deref().map_or_else(
                || "repository selected no tracked paths".to_string(),
                |scope| format!("--scope {scope:?} selected no tracked paths"),
            ),
        )
        .at("/scope")
        .with_details(json!({"scope": normalized_scope}))
        .into());
    }
    let tracked = tracked_paths.len();
    let effective_config = config_result
        .as_ref()
        .cloned()
        .unwrap_or_else(|_| config::default_config());
    let estimate = crate::estimate::build(repo_root, &tracked_paths, &effective_config);
    let resource_status = if estimate.estimated_peak_memory_bytes > estimate.memory_budget_bytes {
        "over_memory_budget"
    } else {
        "within_budget"
    };
    let bundle_path = args.bundle.as_ref().map(|output| {
        if output.is_absolute() {
            output.clone()
        } else {
            repo_root.join(output)
        }
    });
    let mut diagnostics = Vec::new();
    if let Err(error) = &config_result {
        diagnostics.push(json!({"code":"invalid_configuration","severity":"error","detail": crate::text::visible_controls(&format!("{error:#}"))}));
    }
    if report_status == "invalid" {
        diagnostics.push(json!({"code":"invalid_latest_report","severity":"error","detail":"The latest report does not satisfy the supported report contract."}));
    } else if report_status == "missing" {
        diagnostics.push(json!({"code":"latest_report_missing","severity":"notice","detail":"No latest report exists yet."}));
    }
    if repo.is_shallow {
        diagnostics.push(json!({"code":"shallow_history","severity":"warning","detail":"Git history evidence is incomplete."}));
    }
    if resource_status == "over_memory_budget" {
        diagnostics.push(json!({"code":"estimated_memory_budget_exceeded","severity":"error","detail":format!("Estimated peak memory is {} bytes for a {} byte budget.", estimate.estimated_peak_memory_bytes, estimate.memory_budget_bytes)}));
    }
    if tracked_paths.is_empty() {
        diagnostics.push(json!({"code":"no_tracked_paths","severity":"notice","detail":"The repository has no committed tracked paths yet. Create the first commit, then run git slop find."}));
    }
    let scan_ready = config_result.is_ok()
        && resource_status != "over_memory_budget"
        && !tracked_paths.is_empty();
    let report_available = report_status == "compatible";
    let diagnostic = json!({
        "schema_version": 1,
        "command": "doctor",
        "status": if config_result.is_err() || report_status == "invalid" || resource_status == "over_memory_budget" { "error" } else if !scan_ready || !report_available || repo.is_shallow { "not_ready" } else { "ready" },
        "scan_ready": scan_ready,
        "report_available": report_available,
        "repository": {"name": repo.repo_name, "branch": repo.branch, "shallow": repo.is_shallow, "detached": repo.detached_head, "clean": repo.worktree_clean},
        "config": {"status": if config_result.is_err() { "invalid" } else if config_exists { "valid" } else { "using_defaults" }, "path": config::config_path(repo_root)},
        "report": {"status": report_status, "path": report_path},
        "estimate": estimate,
        "resource_status": resource_status,
        "diagnostics": diagnostics,
        "bundle_path": bundle_path,
        "recovery": {
            "config": "Run git slop config validate, then correct the reported key or run git slop config migrate.",
            "report": "Run git slop find to replace a missing or incompatible latest report.",
            "shallow": "Fetch full history or rerun find with --allow-shallow to acknowledge incomplete evidence."
            ,"unborn": "Create the first commit with tracked files, then run git slop find."
        }
    });
    if matches!(args.format, DoctorFormat::Json) {
        print_text(&render_json(&diagnostic)?);
    } else {
        println!("Git Slop doctor");
        println!("- git: available");
        println!("- repository: {}", repo.repo_name);
        println!(
            "- branch: {}",
            repo.branch.as_deref().unwrap_or(if repo.detached_head {
                "detached HEAD"
            } else {
                "unborn branch (no commits)"
            })
        );
        println!(
            "- history: {}",
            if repo.is_shallow {
                "shallow (incomplete)"
            } else {
                "complete"
            }
        );
        println!(
            "- worktree: {} (staged={}, modified={}, untracked={})",
            if repo.worktree_clean {
                "clean"
            } else {
                "dirty"
            },
            repo.staged_change_count,
            repo.modified_tracked_file_count,
            repo.untracked_file_count
        );
        println!(
            "- config: {}",
            if config_result.is_ok() {
                if config_exists {
                    "valid"
                } else {
                    "using built-in defaults"
                }
            } else {
                "invalid"
            }
        );
        println!("- report: {report_status}");
        println!(
            "- preflight: {tracked} tracked files; peak memory ~{} MiB; cache ~{} MiB; report ~{} MiB; time ~{}s; inodes ~{}",
            estimate.estimated_peak_memory_bytes.div_ceil(1024 * 1024),
            estimate.estimated_cache_bytes.div_ceil(1024 * 1024),
            estimate.estimated_report_bytes.div_ceil(1024 * 1024),
            estimate.estimated_seconds,
            estimate.estimated_inode_count,
        );
        if repo.is_shallow {
            println!("- recovery: fetch full history, or explicitly use --allow-shallow");
        }
        if config_result.is_err() {
            println!("- recovery: run `git slop config validate` and correct the reported key");
        }
        if report_status != "compatible" {
            println!("- recovery: run `git slop find` to produce a compatible latest report");
        }
    }
    if let Some(output) = bundle_path {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let config_digest = config_result
            .as_ref()
            .ok()
            .and_then(|value| serde_json::to_vec(value).ok())
            .map(|bytes| hex::encode(sha2::Sha256::digest(bytes)));
        let payload = json!({
            "schema_version": 1,
            "git_slop_version": VERSION,
            "repository": {"name": repo.repo_name, "shallow": repo.is_shallow, "detached": repo.detached_head, "clean": repo.worktree_clean, "staged": repo.staged_change_count, "modified": repo.modified_tracked_file_count, "untracked_count": repo.untracked_file_count},
            "config_digest": config_digest,
            "report_status": report_status,
            "diagnostics": diagnostics,
            "estimate": estimate,
            "privacy": {"source_included": false, "raw_tokens_included": false, "absolute_paths_included": false, "author_identities_included": false, "credentials_included": false}
        });
        fs::write(&output, render_json(&payload)?)?;
        if matches!(args.format, DoctorFormat::Json) {
            eprintln!("Wrote redacted diagnostic bundle to {}.", output.display());
        } else {
            println!("Wrote redacted diagnostic bundle to {}.", output.display());
        }
    }
    Ok(
        if config_result.is_err()
            || report_status == "invalid"
            || resource_status == "over_memory_budget"
        {
            2
        } else {
            0
        },
    )
}
