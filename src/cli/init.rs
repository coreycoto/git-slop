#[derive(Default)]
struct StatePromotion {
    moved: usize,
    retained: usize,
    stale: bool,
}

fn move_state_tree(source: &Path, destination: &Path, result: &mut StatePromotion) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            move_state_tree(&from, &to, result)?;
            if fs::read_dir(&from)?.next().is_none() {
                fs::remove_dir(&from)?;
            }
        } else if to.exists() {
            result.retained += 1;
        } else {
            fs::rename(&from, &to)?;
            result.moved += 1;
        }
    }
    Ok(())
}

fn first_run_state_is_current(repo_root: &Path) -> bool {
    config::git_runtime_dir(repo_root)
        .ok()
        .map(|path| path.join("ephemeral/latest/report.json"))
        .filter(|path| path.is_file())
        .and_then(|path| report::load_report(&path).ok())
        .and_then(|report| crate::freshness::evaluate(repo_root, &report).ok())
        .is_some_and(|freshness| freshness.current)
}

fn promote_first_run_state(repo_root: &Path, eligible: bool) -> Result<StatePromotion> {
    let source = config::git_runtime_dir(repo_root)?.join("ephemeral");
    let report_path = source.join("latest/report.json");
    if !report_path.is_file() {
        return Ok(StatePromotion::default());
    }
    if !eligible {
        return Ok(StatePromotion {
            stale: true,
            ..StatePromotion::default()
        });
    }
    let mut result = StatePromotion::default();
    for directory in ["latest", "runs", "cache"] {
        move_state_tree(
            &source.join(directory),
            &config::slop_dir(repo_root).join(directory),
            &mut result,
        )?;
    }
    if source.is_dir() && fs::read_dir(&source)?.next().is_none() {
        fs::remove_dir(source)?;
    }
    Ok(result)
}

fn repository_owned_slop_paths(repo_root: &Path) -> Vec<String> {
    [
        ".slop/config.yaml",
        ".slop/.gitignore",
        ".slop/policies.yaml",
        ".slop/policy-lock.json",
    ]
    .into_iter()
    .filter(|path| repo_root.join(path).is_file())
    .map(str::to_string)
    .collect()
}

fn stage_command(paths: &[String]) -> Option<String> {
    (!paths.is_empty()).then(|| format!("git add {}", paths.join(" ")))
}

fn check_adoption_payload(repo_root: &Path, args: &InitArgs) -> (Value, bool) {
    let status = config::adoption_status(repo_root);
    let ready = if args.gitignore_only {
        status.gitignore_exists && status.missing_ignore_entries.is_empty()
    } else {
        status.ready()
    };
    let config_status = if args.gitignore_only {
        "skipped"
    } else if !status.config_exists {
        "missing"
    } else if status.config_valid {
        "valid"
    } else {
        "invalid"
    };
    let gitignore_status = if !status.gitignore_exists {
        "missing"
    } else if status.missing_ignore_entries.is_empty() {
        "current"
    } else {
        "repair_needed"
    };
    (
        json!({
            "config": {
                "path": ".slop/config.yaml",
                "status": config_status,
            },
            "gitignore": {
                "path": ".slop/.gitignore",
                "status": gitignore_status,
                "missing_entries": status.missing_ignore_entries,
            }
        }),
        ready,
    )
}

fn run_init(repo_root: &Path, args: InitArgs) -> Result<i32> {
    if args.check {
        let (adoption, ready) = check_adoption_payload(repo_root, &args);
        let staging_paths = repository_owned_slop_paths(repo_root);
        let staging_command = stage_command(&staging_paths);
        let repair_suffix = if args.gitignore_only {
            " --gitignore-only"
        } else {
            ""
        };
        let next_actions = if ready {
            vec!["git slop find".to_string()]
        } else {
            vec![
                format!("git slop init --repair{repair_suffix}"),
                format!("git slop init --check{repair_suffix}"),
            ]
        };
        let receipt = json!({
            "schema_version": 1,
            "command": "init",
            "mode": "check",
            "status": if ready { "ready" } else { "repair_needed" },
            "applied": false,
            "gitignore_only": args.gitignore_only,
            "mutation_class": "none",
            "durable_mutation": false,
            "actions": [],
            "changed_paths": [],
            "backups": [],
            "adoption": adoption.clone(),
            "promotion": {
                "status": "not_applicable",
                "moved_files": 0,
                "retained_files": 0,
            },
            "staging": {
                "required": false,
                "paths": staging_paths.clone(),
                "command": staging_command.clone(),
            },
            "rollback": {
                "status": "not_applicable",
                "guidance": "No changes were made.",
            },
            "next_actions": next_actions,
        });
        if args.format == InitFormat::Json {
            print_text(&render_json(&receipt)?);
            return Ok(if ready { 0 } else { 1 });
        }
        println!(
            "Adoption status: {}.",
            if ready { "ready" } else { "repair needed" }
        );
        if !args.gitignore_only {
            println!(
                "- config: {}",
                adoption["config"]["status"].as_str().unwrap_or("invalid")
            );
        }
        println!(
            "- ignore rules: {}",
            if adoption["gitignore"]["status"] == "missing" {
                "missing".to_string()
            } else if adoption["gitignore"]["status"] == "current" {
                "current".to_string()
            } else {
                format!(
                    "missing {}",
                    adoption["gitignore"]["missing_entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
        if !ready {
            println!(
                "Next: run `git slop init --repair{repair_suffix}`, then `git slop init --check{repair_suffix}`."
            );
        }
        return Ok(if ready { 0 } else { 1 });
    }

    let was_ready = config::adoption_status(repo_root).ready();
    let first_run_current = !was_ready && first_run_state_is_current(repo_root);
    let result = config::initialize(repo_root, args.force, args.repair, args.gitignore_only)?;
    let mut changed_paths = Vec::new();
    let mut actions = Vec::new();
    if !args.gitignore_only {
        actions.push(json!({"path": ".slop/config.yaml", "action": result.config.clone()}));
        if matches!(result.config.as_str(), "written" | "replaced") {
            changed_paths.push(".slop/config.yaml".to_string());
        }
    }
    actions.push(json!({"path": ".slop/.gitignore", "action": result.gitignore.clone()}));
    if matches!(result.gitignore.as_str(), "written" | "replaced")
        || result.gitignore.starts_with("repaired")
    {
        changed_paths.push(".slop/.gitignore".to_string());
    }
    let promotion = if !was_ready && !args.gitignore_only {
        promote_first_run_state(repo_root, first_run_current)?
    } else {
        StatePromotion::default()
    };
    let promotion_status = if was_ready || args.gitignore_only {
        "not_applicable"
    } else if promotion.stale {
        "stale_retained"
    } else if promotion.moved > 0 && promotion.retained > 0 {
        "promoted_with_conflicts"
    } else if promotion.moved > 0 {
        "promoted"
    } else if promotion.retained > 0 {
        "conflicts_retained"
    } else {
        "no_state"
    };
    let backups = result
        .backups
        .iter()
        .map(|path| relative_display(path, repo_root))
        .collect::<Vec<_>>();
    let staging_paths = repository_owned_slop_paths(repo_root);
    let staging_command = stage_command(&staging_paths);
    let rollback_status = if backups.is_empty() {
        "version_control_or_remove_new_files"
    } else {
        "backups_available"
    };
    let rollback_guidance = if backups.is_empty() {
        "Review the changed paths, then restore tracked files from version control or remove newly created files."
    } else {
        "Review the ignored .bak files and restore the intended prior content before deleting any backup."
    };
    let mode = if args.force {
        "force"
    } else if args.repair {
        "repair"
    } else {
        "initialize"
    };
    let mut next_actions = Vec::new();
    if let Some(command) = &staging_command {
        next_actions.push(command.clone());
        next_actions.push("review and commit the staged repository-owned .slop files".to_string());
    }
    next_actions.push("git slop find".to_string());
    let receipt = json!({
        "schema_version": 1,
        "command": "init",
        "mode": mode,
        "status": "initialized",
        "applied": true,
        "gitignore_only": args.gitignore_only,
        "mutation_class": "repository_adoption",
        "durable_mutation": !changed_paths.is_empty() || promotion.moved > 0,
        "actions": actions,
        "changed_paths": changed_paths,
        "backups": backups,
        "adoption": check_adoption_payload(repo_root, &args).0,
        "promotion": {
            "status": promotion_status,
            "moved_files": promotion.moved,
            "retained_files": promotion.retained,
        },
        "staging": {
            "required": !staging_paths.is_empty(),
            "paths": staging_paths.clone(),
            "command": staging_command.clone(),
        },
        "rollback": {
            "status": rollback_status,
            "guidance": rollback_guidance,
        },
        "next_actions": next_actions,
    });
    if args.format == InitFormat::Json {
        print_text(&render_json(&receipt)?);
        return Ok(0);
    }
    if !args.gitignore_only {
        println!(
            "Initialized {} ({}).",
            relative_display(&config::config_path(repo_root), repo_root),
            result.config
        );
    }
    println!(
        "Initialized {} ({}).",
        relative_display(&config::slop_dir(repo_root).join(".gitignore"), repo_root),
        result.gitignore
    );
    println!("Ensured .slop/latest/, .slop/runs/, and .slop/cache/ exist.");
    for backup in &result.backups {
        println!(
            "Recovery backup: {}.",
            relative_display(backup, repo_root)
        );
    }
    if !was_ready && !args.gitignore_only {
        if promotion.stale {
            println!(
                "Retained Git-private first-run state because its report is stale; run `git slop find` after committing adoption."
            );
        } else if promotion.moved > 0 {
            println!(
                "Promoted {} compatible first-run state file(s) into .slop/.",
                promotion.moved
            );
            if promotion.retained > 0 {
                println!(
                    "Retained {} conflicting Git-private state file(s) for explicit review.",
                    promotion.retained
                );
            }
        }
    }
    let adoption_paths = staging_paths
        .iter()
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>()
        .join(", ");
    println!("Next: review and commit {adoption_paths}.");
    if let Some(command) = stage_command(&staging_paths) {
        println!("Stage adoption with `{command}`.");
    }
    println!(
        "Then run `git slop find` (or `git slop find --ephemeral` for a disposable scan)."
    );
    Ok(0)
}
