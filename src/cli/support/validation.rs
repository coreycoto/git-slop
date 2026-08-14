fn require_current_report(
    repo_root: &Path,
    report: &Value,
) -> Result<crate::freshness::ReportFreshness> {
    let freshness = crate::freshness::evaluate(repo_root, report)?;
    if freshness.current {
        return Ok(freshness);
    }
    Err(ClassifiedError::new(
        ErrorKind::Contract,
        "stale_report",
        format!(
            "Report is valid but stale: {}. Run `git slop find` and retry.",
            freshness.reason_codes()
        ),
    )
    .at("/report/freshness")
    .with_details(serde_json::to_value(&freshness)?)
    .into())
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
        .unwrap_or_else(|| match input.trim() {
            "" => ".".to_string(),
            trimmed => trimmed.to_string(),
        })
}

fn usage_error(error: impl std::fmt::Display) -> Result<i32> {
    Err(ClassifiedError::new(ErrorKind::Contract, "invalid_argument", error).into())
}

fn argument_error(
    pointer: &str,
    flag: &str,
    error: impl std::fmt::Display,
    actual: impl serde::Serialize,
) -> Result<i32> {
    Err(ClassifiedError::new(ErrorKind::Contract, "invalid_argument", error)
        .at(pointer)
        .with_details(json!({"flag": flag, "actual": actual}))
        .into())
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
