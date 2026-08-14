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
    let loaded = loaded.ok_or_else(|| -> anyhow::Error {
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
    })?;
    if explicit_report.is_none() && !repo_root.as_os_str().is_empty() {
        match crate::freshness::evaluate(repo_root, &loaded.0) {
            Ok(freshness) if !freshness.current => eprintln!(
                "git-slop: warning: latest report is stale ({}); run `git slop find`.",
                freshness.reason_codes()
            ),
            Err(error) => eprintln!(
                "git-slop: warning: latest report currentness could not be checked: {error:#}"
            ),
            Ok(_) => {}
        }
    }
    Ok(loaded)
}
