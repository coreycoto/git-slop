fn advice_directory_size(path: &Path) -> Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        bail!(
            "advice state must not contain symbolic links: {}",
            path.display()
        );
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        total = total.saturating_add(advice_directory_size(&entry?.path())?);
    }
    Ok(total)
}
#[cfg(unix)]
fn advice_permissions_private(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o077 != 0 {
        return Ok(false);
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            if !advice_permissions_private(&entry?.path())? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[cfg(not(unix))]
fn advice_permissions_private(_path: &Path) -> Result<bool> {
    Ok(true)
}

pub fn state_status(state_root: &Path, current_report: Option<&Value>) -> Result<Value> {
    let root = state_root.join("advice");
    if !root.exists() {
        return Ok(json!({
            "status": "missing",
            "latest": "missing",
            "retained_runs": 0,
            "retained_bytes": 0,
            "private_permissions": true,
            "recovery_entries": 0,
            "retention_command": "git slop prune --dry-run"
        }));
    }
    if root
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        bail!("advice state root must not be a symbolic link");
    }
    let recovery_entries = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name == ".latest.backup" || name.starts_with(".advice-latest-"))
        })
        .count();
    let runs = root.join("runs");
    let retained = if runs.is_dir() {
        fs::read_dir(&runs)?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| !name.starts_with('.'))
            })
            .map(|entry| {
                let bytes = advice_directory_size(&entry.path())?;
                Ok(bytes)
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let latest_path = root.join("latest/advice.json");
    let markdown_path = root.join("latest/advice.md");
    let latest = if !latest_path.exists() && !markdown_path.exists() {
        "missing"
    } else if !latest_path.is_file() || !markdown_path.is_file() {
        "invalid"
    } else {
        let bytes = super::io::read_bounded(
            &latest_path,
            super::io::MAX_ADVICE_ARTIFACT_BYTES,
            "latest advice artifact",
        );
        bytes
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(Into::into))
            .and_then(|artifact| {
                validate_artifact_contract(&artifact)?;
                validate_artifact_semantics(&artifact)?;
                if let Some(report) = current_report {
                    let current = sha256(serde_json::to_vec(report)?);
                    if artifact
                        .pointer("/report/canonical_sha256")
                        .and_then(Value::as_str)
                        != Some(current.as_str())
                    {
                        return Ok("stale");
                    }
                }
                Ok("valid")
            })
            .unwrap_or("invalid")
    };
    let private_permissions = advice_permissions_private(&root)?;
    let status = if recovery_entries > 0 {
        "recovery_required"
    } else if !private_permissions {
        "insecure_permissions"
    } else {
        latest
    };
    Ok(json!({
        "status": status,
        "latest": latest,
        "retained_runs": retained.len(),
        "retained_bytes": retained.into_iter().sum::<u64>(),
        "private_permissions": private_permissions,
        "recovery_entries": recovery_entries,
        "retention_command": "git slop prune --dry-run"
    }))
}
