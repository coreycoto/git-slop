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
    if !adoption.ready() && (args.ephemeral || auto_ephemeral) {
        config::mark_active_state(repo_root, false)?;
    } else if persistent_unadopted {
        config::mark_active_state(repo_root, true)?;
    }
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
