fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if let Err(error) = (|| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()
    })() {
        let _ = fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}
pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn write_pair(directory: &Path, artifact: &Value, markdown: &str) -> Result<()> {
    ensure_private_directory(directory)?;
    let json = serde_json::to_string_pretty(artifact)? + "\n";
    write_private_file(&directory.join("advice.json"), json.as_bytes())?;
    if let Err(error) = write_private_file(&directory.join("advice.md"), markdown.as_bytes()) {
        let _ = fs::remove_file(directory.join("advice.json"));
        return Err(error);
    }
    sync_directory(directory)?;
    Ok(())
}

fn remove_stale_advice_temporaries(directory: &Path, prefix: &str) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(prefix))
        {
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn recover_advice_state(root: &Path, runs: &Path) -> Result<()> {
    let latest = root.join("latest");
    let backup = root.join(".latest.backup");
    if backup.exists() {
        if latest.exists() {
            fs::remove_dir_all(&backup)?;
        } else {
            fs::rename(&backup, &latest)?;
        }
    }
    remove_stale_advice_temporaries(root, ".advice-latest-")?;
    remove_stale_advice_temporaries(runs, ".advice-run-")?;
    sync_directory(root)?;
    sync_directory(runs)?;
    Ok(())
}

pub fn write_artifacts(repo_root: &Path, run: &AdviceRun) -> Result<(PathBuf, PathBuf)> {
    let root = crate::config::active_state_dir(repo_root)?.join("advice");
    ensure_private_directory(&root)?;
    let lock_path = root.join(".write.lock");
    let mut lock_options = fs::OpenOptions::new();
    lock_options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.mode(0o600);
    }
    let lock = lock_options.open(&lock_path)?;
    fs2::FileExt::lock_exclusive(&lock)?;
    let digest = run
        .artifact
        .get("response_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("advice artifact is missing response_sha256"))?;
    let generated = run
        .artifact
        .get("generated_at")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .replace([':', '+'], "-");
    let run_id = format!("{generated}-{}", &digest[..16]);
    let runs = root.join("runs");
    ensure_private_directory(&runs)?;
    recover_advice_state(&root, &runs)?;
    let run_dir = runs.join(&run_id);
    if run_dir.exists() {
        bail!("advice run already exists: {}", run_dir.display());
    }
    let temporary_run = tempfile::Builder::new()
        .prefix(".advice-run-")
        .tempdir_in(&runs)?;
    write_pair(temporary_run.path(), &run.artifact, &run.markdown)?;
    fs::rename(temporary_run.path(), &run_dir)?;
    sync_directory(&runs)?;

    let latest = root.join("latest");
    let temporary_latest = tempfile::Builder::new()
        .prefix(".advice-latest-")
        .tempdir_in(&root)?;
    write_pair(temporary_latest.path(), &run.artifact, &run.markdown)?;
    let backup = root.join(".latest.backup");
    if latest.exists() {
        fs::rename(&latest, &backup)?;
        sync_directory(&root)?;
    }
    if let Err(error) = fs::rename(temporary_latest.path(), &latest) {
        if backup.exists() {
            let _ = fs::rename(&backup, &latest);
        }
        let _ = sync_directory(&root);
        return Err(error.into());
    }
    sync_directory(&root)?;
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
        sync_directory(&root)?;
    }
    Ok((latest.join("advice.json"), latest.join("advice.md")))
}
