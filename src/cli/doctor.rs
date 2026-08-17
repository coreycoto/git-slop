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
    let adoption_recovery = if adoption_status == "repair_needed" && adoption.config_exists {
        Some("git slop init --repair --gitignore-only")
    } else {
        None
    };
    let optional_adoption_command =
        (adoption_status == "not_adopted").then_some("git slop init");
    let adoption_payload = json!({
        "status": adoption_status,
        "config_exists": adoption.config_exists,
        "config_valid": adoption.config_valid,
        "gitignore_exists": adoption.gitignore_exists,
        "missing_ignore_entries": adoption.missing_ignore_entries,
        "recovery_command": adoption_recovery,
        "optional_adoption_command": optional_adoption_command
    });
    let report_path = default_report_path(repo_root);
    let report_storage = if report_path == durable_report_path(repo_root) {
        "durable"
    } else {
        "git_private_ephemeral"
    };
    let mut report_freshness = None;
    let mut freshness_error = None;
    let mut selected_report = None;
    let report_status = if report_path.exists() {
        match report::load_report(&report_path) {
            Ok(loaded) => {
                let freshness = crate::freshness::evaluate(repo_root, &loaded);
                selected_report = Some(loaded);
                match freshness {
                    Ok(freshness) => {
                        let status = freshness.status;
                        report_freshness = Some(freshness);
                        status
                    }
                    Err(error) => {
                        freshness_error = Some(format!("{error:#}"));
                        "unverified"
                    }
                }
            }
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
    let state_root = config::active_state_dir(repo_root)
        .unwrap_or_else(|_| config::slop_dir(repo_root));
    let advice_state = crate::advice::state_status(&state_root, selected_report.as_ref())
        .unwrap_or_else(|error| {
            json!({
                "status": "invalid",
                "latest": "invalid",
                "retained_runs": 0,
                "retained_bytes": 0,
                "private_permissions": false,
                "recovery_entries": 0,
                "retention_command": "git slop prune --dry-run",
                "detail": crate::text::visible_controls(&format!("{error:#}"))
            })
        });
    let detector_cache = writable_cache_probe(&state_root);
    let policy_cache = crate::policy::policy_home()
        .map(|path| writable_cache_probe(&path))
        .unwrap_or_else(|error| {
            json!({"status":"unavailable","writable":false,"detail":format!("policy cache could not be resolved: {error}")})
        });
    let detector_cache_writable = detector_cache["writable"].as_bool() == Some(true);
    let advisor_gate = crate::advice::release_gate();
    let advisor_gate_valid = advisor_gate.is_ok();
    let advisor_payload = match &advisor_gate {
        Ok(gate) => {
            let inference_status = if gate.public_inference_enabled {
                "enabled"
            } else if gate.recommendation == "defer" {
                "deferred"
            } else {
                "disabled"
            };
            json!({
                "status": "available",
                "provider_free_status": "available",
                "inference_status": inference_status,
                "default_mode": "provider_free_markdown",
                "machine_context_mode": "provider_free_json",
                "provider_free_context_available": true,
                "model_required_for_ordinary_use": false,
                "public_inference_enabled": gate.public_inference_enabled,
                "recommendation": gate.recommendation,
                "decision_record": gate.decision_record,
                "decision_record_url": format!(
                    "https://github.com/coreycoto/git-slop/blob/v{VERSION}/{}",
                    gate.decision_record
                ),
                "benchmark_feature_compiled": cfg!(feature = "advisor-inference-benchmark"),
                "state": advice_state.clone(),
            })
        }
        Err(error) => json!({
            "status": "invalid_release_gate",
            "provider_free_status": "available",
            "inference_status": "unavailable",
            "default_mode": "provider_free_markdown",
            "machine_context_mode": "provider_free_json",
            "provider_free_context_available": true,
            "model_required_for_ordinary_use": false,
            "public_inference_enabled": false,
            "detail": crate::text::visible_controls(&format!("{error:#}")),
            "benchmark_feature_compiled": cfg!(feature = "advisor-inference-benchmark"),
            "state": advice_state.clone(),
        }),
    };
    let bundle_path = doctor_bundle_path(repo_root, args.bundle.as_deref());
    let mut diagnostics = Vec::new();
    if adoption_status == "not_adopted" {
        diagnostics.push(json!({"code":"repository_not_adopted","severity":"notice","detail":"Repository adoption files are absent; defaults and Git-private state remain ready for ordinary scans.","optional_command":optional_adoption_command}));
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
    if !detector_cache_writable {
        diagnostics.push(json!({"code":"detector_cache_not_writable","severity":"error","detail":detector_cache.get("detail")}));
    }
    if policy_cache["writable"].as_bool() != Some(true) {
        diagnostics.push(json!({"code":"policy_cache_not_writable","severity":"warning","detail":policy_cache.get("detail"),"recovery_command":"Set GIT_SLOP_POLICY_HOME to a private writable directory before installing policy packs."}));
    }
    if let Err(error) = &advisor_gate {
        diagnostics.push(json!({"code":"advisor_release_gate_invalid","severity":"error","detail":crate::text::visible_controls(&format!("{error:#}"))}));
    }
    if matches!(
        advice_state["status"].as_str(),
        Some("invalid" | "recovery_required" | "insecure_permissions")
    ) {
        diagnostics.push(json!({
            "code": "advisor_state_requires_attention",
            "severity": "warning",
            "detail": format!(
                "Retained advice state is {}; run git slop advise --validate-artifact for evidence validation or git slop prune --dry-run for retention review.",
                advice_state["status"].as_str().unwrap_or("invalid")
            )
        }));
    }
    if tracked_paths.is_empty() {
        diagnostics.push(json!({"code":"no_tracked_paths","severity":"notice","detail":"The repository has no committed tracked paths yet. Commit a tracked file, or run git slop find --allow-empty-scope when an empty report is intentional.","recovery_command":"git slop find --allow-empty-scope"}));
    }
    let scan_ready = config_result.is_ok()
        && detector_cache_writable
        && resource_status != "over_memory_budget"
        && !tracked_paths.is_empty();
    let report_available = matches!(report_status, "current" | "stale" | "unverified");
    let report_current = report_status == "current";
    let doctor_status = if config_result.is_err()
        || report_status == "invalid"
        || resource_status == "over_memory_budget"
        || !detector_cache_writable
        || !advisor_gate_valid
    {
        "error"
    } else if !scan_ready || !report_available || !report_current || repo.is_shallow {
        "not_ready"
    } else {
        "ready"
    };
    let diagnostic = json!({
        "schema_version": 1,
        "command": "doctor",
        "status": doctor_status,
        "scan_ready": scan_ready,
        "report_available": report_available,
        "repository": {"name": repo.repo_name, "branch": repo.branch, "shallow": repo.is_shallow, "detached": repo.detached_head, "clean": repo.worktree_clean},
        "adoption": adoption_payload.clone(),
        "config": {"status": if config_result.is_err() { "invalid" } else if config_exists { "valid" } else { "using_defaults" }, "path": config::config_path(repo_root)},
        "report": {"status": report_status, "path": report_path, "storage": report_storage, "freshness": report_freshness, "freshness_error": freshness_error},
        "estimate": estimate,
        "cache_writability": {"detector_state": detector_cache, "policy_packs": policy_cache},
        "advisor": advisor_payload,
        "resource_status": resource_status,
        "diagnostics": diagnostics,
        "bundle_path": bundle_path,
        "recovery": {
            "config": "Run git slop config validate, then correct the reported key or run git slop config migrate.",
            "report": "Run git slop find to replace a missing, incompatible, or stale latest report.",
            "shallow": "Fetch full history or rerun find with --allow-shallow to acknowledge incomplete evidence.",
            "unborn": "Commit a tracked file, or run git slop find --allow-empty-scope when an empty report is intentional."
        }
    });
    if matches!(args.format, DoctorFormat::Json) {
        print_text(&render_json(&diagnostic)?);
    } else {
        println!("Git Slop doctor");
        println!("- status: {}", doctor_status.replace('_', " "));
        println!(
            "- readiness: scan={} report={} current={}",
            scan_ready, report_available, report_current
        );
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
        println!(
            "- report: {report_status} ({report_storage}, {})",
            relative_display(&report_path, repo_root)
        );
        println!(
            "- preflight: {tracked} tracked files; peak memory ~{} MiB; cache ~{} MiB; report ~{} MiB; time ~{}s cold/~{}s warm; inodes ~{}",
            estimate.estimated_peak_memory_bytes.div_ceil(1024 * 1024),
            estimate.estimated_cache_bytes.div_ceil(1024 * 1024),
            estimate.estimated_report_bytes.div_ceil(1024 * 1024),
            estimate.estimated_seconds_cold,
            estimate.estimated_seconds_warm,
            estimate.estimated_inode_count,
        );
        println!(
            "- writable caches: detector={} policy-packs={}",
            detector_cache["status"].as_str().unwrap_or("unknown"),
            policy_cache["status"].as_str().unwrap_or("unknown")
        );
        println!(
            "- advisor: provider-free={}; inference={}; model required for ordinary use=no ({})",
            advisor_payload["provider_free_status"]
                .as_str()
                .unwrap_or("unavailable"),
            advisor_payload["inference_status"]
                .as_str()
                .unwrap_or("unavailable"),
            advisor_payload["recommendation"]
                .as_str()
                .unwrap_or("invalid release gate")
        );
        if let Some(url) = advisor_payload["decision_record_url"].as_str() {
            println!("- advisor decision: {url}");
        }
        println!(
            "- advisor state: {}; retained runs={} ({} bytes); private permissions={}",
            advice_state["status"].as_str().unwrap_or("invalid"),
            advice_state["retained_runs"].as_u64().unwrap_or(0),
            advice_state["retained_bytes"].as_u64().unwrap_or(0),
            advice_state["private_permissions"]
                .as_bool()
                .unwrap_or(false)
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
        if let Some(command) = optional_adoption_command {
            println!("- optional adoption: run `{command}` when you want durable repository-owned reports");
        }
        if tracked_paths.is_empty() {
            println!(
                "- recovery: commit a tracked file, or run `git slop find --allow-empty-scope` for an intentional empty report"
            );
        } else if report_status != "current" {
            println!("- recovery: run `git slop find` to produce a current latest report");
        }
    }
    if let Some(output) = bundle_path.as_deref() {
        write_doctor_bundle(output, args.format, &config_result, &diagnostic, &repo)?;
    }
    Ok(if config_result.is_err()
        || report_status == "invalid"
        || resource_status == "over_memory_budget"
        || !detector_cache_writable
        || !advisor_gate_valid
        || (args.require_current && report_status != "current")
    {
        2
    } else {
        0
    })
}
