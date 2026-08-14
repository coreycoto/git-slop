fn run_doctor(repo_root: &Path, args: DoctorArgs) -> Result<i32> {
    let repo = git::repo_metadata(repo_root)?;
    let config_result = config::load(repo_root);
    let config_exists = config::config_path(repo_root).is_file();
    let adoption = config::adoption_status(repo_root);
    let adoption_status = if adoption.ready() {
        "ready"
    } else if !adoption.config_exists && !adoption.gitignore_exists {
        "not_adopted"
    } else {
        "repair_needed"
    };
    let adoption_recovery = if adoption_status == "ready" {
        None
    } else if adoption.config_exists {
        Some("git slop init --repair --gitignore-only")
    } else {
        Some("git slop init")
    };
    let adoption_payload = json!({
        "status": adoption_status,
        "config_exists": adoption.config_exists,
        "config_valid": adoption.config_valid,
        "gitignore_exists": adoption.gitignore_exists,
        "missing_ignore_entries": adoption.missing_ignore_entries,
        "recovery_command": adoption_recovery
    });
    let report_path = default_report_path(repo_root);
    let mut report_freshness = None;
    let mut freshness_error = None;
    let report_status = if report_path.exists() {
        match report::load_report(&report_path) {
            Ok(loaded) => match crate::freshness::evaluate(repo_root, &loaded) {
                Ok(freshness) => {
                    let status = freshness.status;
                    report_freshness = Some(freshness);
                    status
                }
                Err(error) => {
                    freshness_error = Some(format!("{error:#}"));
                    "unverified"
                }
            },
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
    if adoption_status == "not_adopted" {
        diagnostics.push(json!({"code":"repository_not_adopted","severity":"notice","detail":"Repository adoption files are absent; defaults remain usable for a first scan.","recovery_command":adoption_recovery}));
    } else if adoption_status == "repair_needed" {
        diagnostics.push(json!({"code":"adoption_repair_needed","severity":"warning","detail":"Repository adoption files are incomplete or their runtime ignore rules are stale.","recovery_command":adoption_recovery}));
    }
    if let Err(error) = &config_result {
        diagnostics.push(json!({"code":"invalid_configuration","severity":"error","detail": crate::text::visible_controls(&format!("{error:#}"))}));
    }
    if report_status == "invalid" {
        diagnostics.push(json!({"code":"invalid_latest_report","severity":"error","detail":"The latest report does not satisfy the supported report contract."}));
    } else if report_status == "missing" {
        diagnostics.push(json!({"code":"latest_report_missing","severity":"notice","detail":"No latest report exists yet."}));
    } else if report_status == "stale" {
        diagnostics.push(json!({"code":"stale_latest_report","severity":"warning","detail":"The latest report is valid but does not match current repository state.","freshness":report_freshness}));
    } else if report_status == "unverified" {
        diagnostics.push(json!({"code":"report_freshness_unverified","severity":"warning","detail":freshness_error}));
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
    let report_available = matches!(report_status, "current" | "stale" | "unverified");
    let report_current = report_status == "current";
    let diagnostic = json!({
        "schema_version": 1,
        "command": "doctor",
        "status": if config_result.is_err() || report_status == "invalid" || resource_status == "over_memory_budget" { "error" } else if !scan_ready || !report_available || !report_current || repo.is_shallow { "not_ready" } else { "ready" },
        "scan_ready": scan_ready,
        "report_available": report_available,
        "repository": {"name": repo.repo_name, "branch": repo.branch, "shallow": repo.is_shallow, "detached": repo.detached_head, "clean": repo.worktree_clean},
        "adoption": adoption_payload.clone(),
        "config": {"status": if config_result.is_err() { "invalid" } else if config_exists { "valid" } else { "using_defaults" }, "path": config::config_path(repo_root)},
        "report": {"status": report_status, "path": report_path, "freshness": report_freshness, "freshness_error": freshness_error},
        "estimate": estimate,
        "resource_status": resource_status,
        "diagnostics": diagnostics,
        "bundle_path": bundle_path,
        "recovery": {
            "config": "Run git slop config validate, then correct the reported key or run git slop config migrate.",
            "report": "Run git slop find to replace a missing, incompatible, or stale latest report.",
            "shallow": "Fetch full history or rerun find with --allow-shallow to acknowledge incomplete evidence.",
            "unborn": "Create the first commit with tracked files, then run git slop find."
        }
    });
    if matches!(args.format, DoctorFormat::Json) {
        print_text(&render_json(&diagnostic)?);
    } else {
        println!("Git Slop doctor");
        println!("- git: available");
        println!("- repository: {}", repo.repo_name);
        println!("- adoption: {adoption_status}");
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
            if repo.worktree_clean { "clean" } else { "dirty" },
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
        if let Some(command) = adoption_recovery {
            println!("- adoption recovery: run `{command}`");
        }
        if report_status != "current" {
            println!("- recovery: run `git slop find` to produce a current latest report");
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
            "adoption": adoption_payload,
            "config_digest": config_digest,
            "report_status": report_status,
            "report_freshness": report_freshness,
            "diagnostics": diagnostics,
            "estimate": estimate,
            "privacy": {"source_included": false, "raw_tokens_included": false, "absolute_paths_included": false, "author_identities_included": false, "credentials_included": false}
        });
        config::write_text_atomically(&output, render_json(&payload)?, false)?;
        if matches!(args.format, DoctorFormat::Json) {
            eprintln!("Wrote redacted diagnostic bundle to {}.", output.display());
        } else {
            println!("Wrote redacted diagnostic bundle to {}.", output.display());
        }
    }
    Ok(if config_result.is_err()
        || report_status == "invalid"
        || resource_status == "over_memory_budget"
        || (args.require_current && report_status != "current")
    {
        2
    } else {
        0
    })
}
