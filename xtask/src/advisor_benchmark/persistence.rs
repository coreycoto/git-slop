fn write_temporary_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = (|| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()
    })() {
        let _ = fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

fn restore_pair_destination(destination: &Path, backup: &Path, had_original: bool) -> Result<()> {
    if destination.exists() {
        fs::remove_file(destination).with_context(|| {
            format!(
                "failed to remove partial benchmark output {}",
                destination.display()
            )
        })?;
    }
    if had_original {
        fs::rename(backup, destination).with_context(|| {
            format!(
                "failed to restore benchmark output {}",
                destination.display()
            )
        })?;
    }
    Ok(())
}

fn restore_pair(
    results_path: &Path,
    results_backup: &Path,
    had_results: bool,
    decision_path: &Path,
    decision_backup: &Path,
    had_decision: bool,
) -> Result<()> {
    let results_restore =
        restore_pair_destination(results_path, results_backup, had_results).err();
    let decision_restore =
        restore_pair_destination(decision_path, decision_backup, had_decision).err();
    match (results_restore, decision_restore) {
        (None, None) => Ok(()),
        (Some(error), None) | (None, Some(error)) => Err(error),
        (Some(results_error), Some(decision_error)) => bail!(
            "benchmark result and decision rollback both failed: {results_error:#}; {decision_error:#}"
        ),
    }
}

fn replacement_error(error: std::io::Error, rollback: Result<()>) -> anyhow::Error {
    match rollback {
        Ok(()) => error.into(),
        Err(rollback_error) => anyhow::anyhow!(
            "benchmark output replacement failed: {error}; rollback also failed: {rollback_error:#}"
        ),
    }
}

fn write_benchmark_pair(
    results_path: &Path,
    result: &Value,
    decision_path: &Path,
    decision: &str,
) -> Result<()> {
    validate_benchmark_result(result)?;
    let results_parent = results_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("benchmark results path has no parent"))?;
    if decision_path.parent() != Some(results_parent) || results_path == decision_path {
        bail!("benchmark results and decision must be distinct files in one directory");
    }
    fs::create_dir_all(results_parent)?;
    for path in [results_path, decision_path] {
        if path.exists() && !path.is_file() {
            bail!("benchmark output destination is not a file: {}", path.display());
        }
    }

    let nonce = format!("{}-{}", std::process::id(), now_ms());
    let results_temporary = results_parent.join(format!(".results-{nonce}.tmp"));
    let decision_temporary = results_parent.join(format!(".decision-{nonce}.tmp"));
    let results_backup = results_parent.join(format!(".results-{nonce}.backup"));
    let decision_backup = results_parent.join(format!(".decision-{nonce}.backup"));
    let results_bytes = serde_json::to_string_pretty(result)? + "\n";
    write_temporary_file(&results_temporary, results_bytes.as_bytes())?;
    if let Err(error) = write_temporary_file(&decision_temporary, decision.as_bytes()) {
        let _ = fs::remove_file(&results_temporary);
        return Err(error);
    }

    let had_results = results_path.exists();
    let had_decision = decision_path.exists();
    if had_results {
        if let Err(error) = fs::rename(results_path, &results_backup) {
            let _ = fs::remove_file(&results_temporary);
            let _ = fs::remove_file(&decision_temporary);
            return Err(error.into());
        }
    }
    if had_decision {
        if let Err(error) = fs::rename(decision_path, &decision_backup) {
            let rollback = restore_pair_destination(results_path, &results_backup, had_results);
            let _ = fs::remove_file(&results_temporary);
            let _ = fs::remove_file(&decision_temporary);
            return Err(replacement_error(error, rollback));
        }
    }
    if let Err(error) = fs::rename(&results_temporary, results_path) {
        let rollback = restore_pair(
            results_path,
            &results_backup,
            had_results,
            decision_path,
            &decision_backup,
            had_decision,
        );
        let _ = fs::remove_file(&results_temporary);
        let _ = fs::remove_file(&decision_temporary);
        return Err(replacement_error(error, rollback));
    }
    if let Err(error) = fs::rename(&decision_temporary, decision_path) {
        let rollback = restore_pair(
            results_path,
            &results_backup,
            had_results,
            decision_path,
            &decision_backup,
            had_decision,
        );
        let _ = fs::remove_file(&decision_temporary);
        return Err(replacement_error(error, rollback));
    }
    if had_results {
        fs::remove_file(results_backup)?;
    }
    if had_decision {
        fs::remove_file(decision_backup)?;
    }
    Ok(())
}
