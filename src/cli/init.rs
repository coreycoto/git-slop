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

fn run_init(repo_root: &Path, args: InitArgs) -> Result<i32> {
    if args.check {
        let status = config::adoption_status(repo_root);
        let ready = if args.gitignore_only {
            status.gitignore_exists && status.missing_ignore_entries.is_empty()
        } else {
            status.ready()
        };
        println!(
            "Adoption status: {}.",
            if ready { "ready" } else { "repair needed" }
        );
        if !args.gitignore_only {
            println!(
                "- config: {}",
                if !status.config_exists {
                    "missing"
                } else if status.config_valid {
                    "valid"
                } else {
                    "invalid"
                }
            );
        }
        println!(
            "- ignore rules: {}",
            if !status.gitignore_exists {
                "missing".to_string()
            } else if status.missing_ignore_entries.is_empty() {
                "current".to_string()
            } else {
                format!("missing {}", status.missing_ignore_entries.join(", "))
            }
        );
        if !ready {
            let suffix = if args.gitignore_only {
                " --gitignore-only"
            } else {
                ""
            };
            println!(
                "Next: run `git slop init --repair{suffix}`, then `git slop init --check{suffix}`."
            );
        }
        return Ok(if ready { 0 } else { 1 });
    }

    let was_ready = config::adoption_status(repo_root).ready();
    let first_run_current = !was_ready && first_run_state_is_current(repo_root);
    let result = config::initialize(repo_root, args.force, args.repair, args.gitignore_only)?;
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
    for backup in result.backups {
        println!(
            "Recovery backup: {}.",
            relative_display(&backup, repo_root)
        );
    }
    if !was_ready && !args.gitignore_only {
        let promotion = promote_first_run_state(repo_root, first_run_current)?;
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
    let adoption_paths = if args.gitignore_only {
        "`.slop/.gitignore`"
    } else {
        "`.slop/config.yaml` and `.slop/.gitignore`"
    };
    let stage_command = if args.gitignore_only {
        "git add .slop/.gitignore"
    } else {
        "git add .slop/config.yaml .slop/.gitignore"
    };
    println!("Next: review and commit {adoption_paths}.");
    println!("Stage adoption with `{stage_command}`.");
    println!(
        "Then run `git slop find` (or `git slop find --ephemeral` for a disposable scan)."
    );
    Ok(0)
}
