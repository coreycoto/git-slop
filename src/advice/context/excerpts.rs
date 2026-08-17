fn resolve_file(repo_root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("unsafe advice context path {relative:?}");
    }
    let canonical_root = repo_root.canonicalize()?;
    let mut candidate = canonical_root.clone();
    for component in path.components() {
        if let Component::Normal(component) = component {
            candidate.push(component);
            if fs::symlink_metadata(&candidate)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                bail!("advice context paths must not traverse symlinks: {relative}");
            }
        }
    }
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("advice context path does not exist: {relative}"))?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
        bail!("advice context path escapes the repository or is not a file: {relative}");
    }
    Ok(canonical)
}

fn is_tracked(repo_root: &Path, relative: &str) -> bool {
    Command::new("git")
        .current_dir(repo_root)
        .args(["ls-files", "--error-unmatch", "--", relative])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn is_ignored(repo_root: &Path, relative: &str) -> bool {
    Command::new("git")
        .current_dir(repo_root)
        .args(["check-ignore", "--no-index", "--quiet", "--", relative])
        .status()
        .is_ok_and(|status| status.success())
}

fn report_file<'a>(report: &'a Value, path: &str) -> Option<&'a Value> {
    report
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|file| file.get("path").and_then(Value::as_str) == Some(path))
}

fn truncate_utf8(text: &str, maximum: usize) -> &str {
    if text.len() <= maximum {
        return text;
    }
    let mut end = maximum;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn excerpt(
    report: &Value,
    repo_root: &Path,
    path: &str,
    kind: &str,
    reason: &str,
    maximum: usize,
) -> Result<Value> {
    if !is_tracked(repo_root, path) {
        bail!("advice context path is not tracked by Git: {path}");
    }
    if is_ignored(repo_root, path) {
        bail!("advice context path is ignored by repository policy: {path}");
    }
    let absolute = resolve_file(repo_root, path)?;
    let bytes = super::io::read_bounded(
        &absolute,
        MAX_SOURCE_FILE_BYTES,
        "advice context file",
    )?;
    if bytes.contains(&0) {
        bail!("binary advice context is not allowed: {path}");
    }
    let text = std::str::from_utf8(&bytes)
        .with_context(|| format!("advice context must be UTF-8: {path}"))?;
    let digest = sha256(&bytes);
    if let Some(expected) = report_file(report, path)
        .and_then(|file| file.get("content_sha256"))
        .and_then(Value::as_str)
    {
        if expected != digest {
            bail!("stale advice evidence: {path} no longer matches report content_sha256");
        }
    }
    let returned = truncate_utf8(text, maximum);
    let truncated = returned.len() < text.len();
    let line_count = returned.lines().count().max(1);
    let excerpt_id = format!(
        "excerpt-{}",
        &sha256(format!("{kind}\0{path}\0{digest}\0{}", returned.len()))[..16]
    );
    Ok(json!({
        "id": excerpt_id,
        "kind": kind,
        "path": path,
        "selection_reason": reason,
        "line_range": {"start": 1, "end": line_count},
        "content_sha256": digest,
        "excerpt_sha256": sha256(returned.as_bytes()),
        "original_bytes": bytes.len(),
        "returned_bytes": returned.len(),
        "truncated": truncated,
        "text": returned,
        "trust": "untrusted_repository_content",
    }))
}

fn collect_reference_ids(value: &Value, key: &str, target: &mut BTreeSet<String>) {
    let plural_key = format!("{key}s");
    let ids_key = format!("{key}_ids");
    match value {
        Value::Object(object) => {
            for (candidate_key, candidate_value) in object {
                if candidate_key == key || candidate_key == &plural_key || candidate_key == &ids_key
                {
                    if let Some(id) = candidate_value.as_str() {
                        target.insert(id.to_string());
                    }
                    push_strings(Some(candidate_value), target);
                }
                collect_reference_ids(candidate_value, key, target);
            }
        }
        Value::Array(values) => {
            for item in values {
                collect_reference_ids(item, key, target);
            }
        }
        _ => {}
    }
}
