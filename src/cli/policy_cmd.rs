fn policy_target_path(repo_root: &Path, target: &str) -> Option<PathBuf> {
    let path = Path::new(target);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    };
    resolved.exists().then_some(resolved)
}

fn print_policy_output(value: &Value, format: PolicyFormat) -> Result<()> {
    match format {
        PolicyFormat::Json => print_text(&serde_json::to_string_pretty(value)?),
        PolicyFormat::Text => print_text(&serde_yaml::to_string(value)?),
    }
    Ok(())
}

fn run_policy(repo_root: &Path, args: PolicyArgs) -> Result<i32> {
    let (value, format) = match args.command {
        PolicyCommand::Init { directory, format } => {
            let directory = resolve_repo_path(repo_root, &directory);
            (crate::policy::init_pack(&directory)?, format)
        }
        PolicyCommand::Validate { target, format } => {
            let path = policy_target_path(repo_root, &target);
            (
                crate::policy::validate_pack_reference(&target, path.as_deref())?,
                format,
            )
        }
        PolicyCommand::Test { target, format } => {
            let path = policy_target_path(repo_root, &target);
            (crate::policy::test_pack(&target, path.as_deref())?, format)
        }
        PolicyCommand::Install {
            source,
            select,
            format,
        } => {
            let source = resolve_repo_path(repo_root, &source);
            (crate::policy::install_pack(repo_root, &source, select)?, format)
        }
        PolicyCommand::Lock { format } => {
            (crate::policy::lock_selected_packs(repo_root)?, format)
        }
        PolicyCommand::List { format } => (crate::policy::list_packs(repo_root)?, format),
        PolicyCommand::Show { target, format } => {
            let path = policy_target_path(repo_root, &target);
            (
                crate::policy::show_pack_or_rule(&target, path.as_deref())?,
                format,
            )
        }
        PolicyCommand::Remove {
            pack_id,
            unselect,
            format,
        } => (
            crate::policy::remove_pack(repo_root, &pack_id, unselect)?,
            format,
        ),
    };
    print_policy_output(&value, format)?;
    Ok(0)
}
