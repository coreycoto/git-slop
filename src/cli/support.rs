fn print_text(value: &str) {
    if value.is_empty() {
        return;
    }
    print!("{value}");
    if !value.ends_with('\n') {
        println!();
    }
}

fn safe_terminal(value: &str) -> String {
    crate::text::visible_controls(value)
}

fn relative_display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn default_report_path(repo_root: &Path) -> PathBuf {
    config::latest_dir(repo_root).join("report.json")
}

fn load_report_at(path: &Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    report::load_report(path).map(Some)
}

fn load_default_report(
    repo_root: &Path,
    explicit_report: Option<&Path>,
) -> Result<Option<(Value, PathBuf)>> {
    let path = explicit_report
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_report_path(repo_root));
    Ok(load_report_at(&path)?.map(|report| (report, path)))
}

fn report_or_missing(repo_root: &Path, explicit_report: Option<&Path>) -> Result<(Value, PathBuf)> {
    let fallback = explicit_report
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_report_path(repo_root));
    let loaded = load_default_report(repo_root, explicit_report).map_err(|error| {
        ClassifiedError::new(ErrorKind::Contract, "report_invalid", format!("{error:#}"))
            .at("/report")
            .with_details(json!({"path": fallback}))
    })?;
    loaded.ok_or_else(|| {
        ClassifiedError::new(
            ErrorKind::Contract,
            "report_not_found",
            format!(
                "Report not found: {}\nRun `git slop find` to generate it.",
                fallback.display()
            ),
        )
        .at("/report")
        .with_details(json!({"path": fallback}))
        .into()
    })
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push(component.as_os_str());
                }
            }
            _ => result.push(component.as_os_str()),
        }
    }
    result
}

fn selector_path(repo_root: &Path, input: &str) -> String {
    let candidate = if Path::new(input).is_absolute() {
        lexical_normalize(Path::new(input))
    } else {
        lexical_normalize(&repo_root.join(input))
    };
    candidate
        .strip_prefix(repo_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                ".".to_string()
            } else {
                trimmed.to_string()
            }
        })
}

fn usage_error(error: impl std::fmt::Display) -> Result<i32> {
    Err(ClassifiedError::new(ErrorKind::Contract, "invalid_argument", error).into())
}

fn ensure_prompt_pack_target(path: &Path) -> Result<()> {
    if path.exists() && !path.is_dir() {
        Err(ClassifiedError::new(
            ErrorKind::Contract,
            "prompt_pack_collision",
            format!("Prompt pack path is not a directory: {}", path.display()),
        )
        .at("/prompt_pack")
        .with_details(json!({"path": path}))
        .into())
    } else {
        Ok(())
    }
}

fn run_init(repo_root: &Path, args: InitArgs) -> Result<i32> {
    let result = config::initialize(repo_root, args.force)?;
    println!(
        "Initialized {} ({}).",
        relative_display(&config::config_path(repo_root), repo_root),
        result.config
    );
    println!(
        "Initialized {} ({}).",
        relative_display(&config::slop_dir(repo_root).join(".gitignore"), repo_root),
        result.gitignore
    );
    println!("Ensured .slop/latest/, .slop/runs/, and .slop/cache/ exist.");
    Ok(0)
}
