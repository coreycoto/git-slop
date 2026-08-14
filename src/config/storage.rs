fn backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("git-slop-state");
    path.with_file_name(format!("{file_name}.bak"))
}

/// Replace a text file without exposing a partially written destination.
/// When requested, copy the previous complete file to a stable `.bak` path.
pub fn write_text_atomically(
    path: &Path,
    contents: impl AsRef<[u8]>,
    backup_existing: bool,
) -> Result<Option<PathBuf>> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("git-slop-state");
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .with_context(|| format!("failed to create {}", temporary.display()))?;
    if let Err(error) = (|| -> Result<()> {
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        Ok(())
    })() {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("failed to write {}", temporary.display()));
    }

    let backup = if backup_existing && path.exists() {
        let backup = backup_path(path);
        fs::copy(path, &backup).with_context(|| {
            format!(
                "failed to create recovery backup {} from {}",
                backup.display(),
                path.display()
            )
        })?;
        Some(backup)
    } else {
        None
    };
    let previous = parent.join(format!(".{file_name}.{}.previous", std::process::id()));
    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &previous).with_context(|| {
            format!("failed to prepare atomic replacement for {}", path.display())
        })?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        if had_previous {
            let _ = fs::rename(&previous, path);
        }
        return Err(error).with_context(|| format!("failed to replace {}", path.display()));
    }
    if had_previous {
        fs::remove_file(&previous)
            .with_context(|| format!("failed to remove {}", previous.display()))?;
    }
    Ok(backup)
}
