fn writable_cache_probe(target: &Path) -> Value {
    let mut probe_root = target;
    while !probe_root.exists() {
        let Some(parent) = probe_root.parent() else {
            return json!({"status":"blocked","writable":false,"detail":"no existing parent directory is available"});
        };
        probe_root = parent;
    }
    if !probe_root.is_dir() {
        return json!({"status":"blocked","writable":false,"detail":"the cache path or its nearest existing parent is not a directory"});
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let probe = probe_root.join(format!(
        ".git-slop-write-probe-{}-{nonce}",
        std::process::id()
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            match fs::remove_file(&probe) {
                Ok(()) => json!({"status":"writable","writable":true}),
                Err(error) => json!({"status":"blocked","writable":false,"detail":format!("write probe succeeded but cleanup failed: {error}")}),
            }
        }
        Err(error) => json!({"status":"blocked","writable":false,"detail":format!("write probe failed: {error}")}),
    }
}

fn doctor_bundle_path(repo_root: &Path, requested: Option<&Path>) -> Option<PathBuf> {
    requested.map(|output| {
        if output == Path::new("__git_slop_active_state_bundle__") {
            config::active_state_dir(repo_root)
                .unwrap_or_else(|_| config::slop_dir(repo_root))
                .join("diagnostic-bundle.json")
        } else if output.is_absolute() {
            output.to_path_buf()
        } else {
            repo_root.join(output)
        }
    })
}

fn write_doctor_bundle(
    output: &Path,
    format: DoctorFormat,
    config_result: &Result<Value>,
    diagnostic: &Value,
    repo: &crate::model::RepoMetadata,
) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let config_digest = config_result
        .as_ref()
        .ok()
        .and_then(|value| serde_json::to_vec(value).ok())
        .map(|bytes| hex::encode(sha2::Sha256::digest(bytes)));
    let report = &diagnostic["report"];
    let caches = &diagnostic["cache_writability"];
    let payload = json!({
        "schema_version": 1,
        "git_slop_version": VERSION,
        "repository": {
            "name": repo.repo_name,
            "shallow": repo.is_shallow,
            "detached": repo.detached_head,
            "clean": repo.worktree_clean,
            "staged": repo.staged_change_count,
            "modified": repo.modified_tracked_file_count,
            "untracked_count": repo.untracked_file_count
        },
        "adoption": diagnostic["adoption"].clone(),
        "config_digest": config_digest,
        "report_status": report["status"].clone(),
        "report_freshness": report["freshness"].clone(),
        "diagnostics": diagnostic["diagnostics"].clone(),
        "estimate": diagnostic["estimate"].clone(),
        "cache_writability": {
            "detector_state_writable": caches["detector_state"]["writable"].clone(),
            "policy_packs_writable": caches["policy_packs"]["writable"].clone()
        },
        "privacy": {"source_included": false, "raw_tokens_included": false, "absolute_paths_included": false, "author_identities_included": false, "credentials_included": false}
    });
    config::write_text_atomically(output, render_json(&payload)?, false)?;
    if matches!(format, DoctorFormat::Json) {
        eprintln!("Wrote redacted diagnostic bundle to {}.", output.display());
    } else {
        println!("Wrote redacted diagnostic bundle to {}.", output.display());
    }
    Ok(())
}
