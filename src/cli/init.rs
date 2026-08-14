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
