#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BenchmarkPairTransaction {
    schema_version: u64,
    results_file: String,
    decision_file: String,
    results_temporary: String,
    decision_temporary: String,
    results_backup: String,
    decision_backup: String,
    had_results: bool,
    had_decision: bool,
}

fn sync_benchmark_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn write_secure_temporary(parent: &Path, prefix: &str, bytes: &[u8]) -> Result<PathBuf> {
    use std::io::Write;

    let mut temporary = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    let (_, path) = temporary.keep()?;
    Ok(path)
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
    } else if backup.exists() {
        fs::remove_file(backup)?;
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

fn transaction_member(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    if name.is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(std::path::Component::Normal(_)))
    {
        bail!("benchmark transaction contains an unsafe path");
    }
    Ok(parent.join(path))
}

fn remove_if_present(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn recover_benchmark_pair(
    parent: &Path,
    results_path: &Path,
    decision_path: &Path,
) -> Result<bool> {
    let journal = parent.join(".benchmark-pair.transaction.json");
    if !journal.exists() {
        return Ok(false);
    }
    let transaction: BenchmarkPairTransaction = serde_json::from_slice(&read_bounded(
        &journal,
        MAX_BENCHMARK_CONFIG_BYTES,
        "benchmark pair transaction",
    )?)?;
    if transaction.schema_version != 1
        || results_path.file_name().and_then(|name| name.to_str())
            != Some(transaction.results_file.as_str())
        || decision_path.file_name().and_then(|name| name.to_str())
            != Some(transaction.decision_file.as_str())
    {
        bail!("benchmark pair transaction does not match the requested output pair");
    }
    let results_temporary = transaction_member(parent, &transaction.results_temporary)?;
    let decision_temporary = transaction_member(parent, &transaction.decision_temporary)?;
    let results_backup = transaction_member(parent, &transaction.results_backup)?;
    let decision_backup = transaction_member(parent, &transaction.decision_backup)?;
    let fully_installed = results_path.is_file()
        && decision_path.is_file()
        && !results_temporary.exists()
        && !decision_temporary.exists();
    if fully_installed {
        remove_if_present(&results_backup)?;
        remove_if_present(&decision_backup)?;
    } else {
        restore_pair(
            results_path,
            &results_backup,
            transaction.had_results,
            decision_path,
            &decision_backup,
            transaction.had_decision,
        )?;
        remove_if_present(&results_temporary)?;
        remove_if_present(&decision_temporary)?;
    }
    fs::remove_file(&journal)?;
    sync_benchmark_directory(parent)?;
    Ok(true)
}

fn write_transaction(parent: &Path, transaction: &BenchmarkPairTransaction) -> Result<()> {
    let journal = parent.join(".benchmark-pair.transaction.json");
    let bytes = serde_json::to_vec(transaction)?;
    let temporary = write_secure_temporary(parent, ".benchmark-transaction-", &bytes)?;
    if let Err(error) = fs::hard_link(&temporary, &journal) {
        let _ = fs::remove_file(temporary);
        return Err(error.into());
    }
    fs::remove_file(&temporary)?;
    sync_benchmark_directory(parent)?;
    Ok(())
}

fn replacement_error(error: anyhow::Error, rollback: Result<bool>) -> anyhow::Error {
    match rollback {
        Ok(_) => error,
        Err(rollback_error) => anyhow::anyhow!(
            "benchmark output replacement failed: {error:#}; recovery also failed: {rollback_error:#}"
        ),
    }
}

fn write_benchmark_pair(
    results_path: &Path,
    result: &Value,
    decision_path: &Path,
    decision: &str,
    replace_existing: bool,
) -> Result<()> {
    validate_benchmark_result(result)?;
    let results_parent = results_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("benchmark results path has no parent"))?;
    if decision_path.parent() != Some(results_parent) || results_path == decision_path {
        bail!("benchmark results and decision must be distinct files in one directory");
    }
    fs::create_dir_all(results_parent)?;
    recover_benchmark_pair(results_parent, results_path, decision_path)?;
    for path in [results_path, decision_path] {
        if path.exists() && !path.is_file() {
            bail!("benchmark output destination is not a file: {}", path.display());
        }
    }
    if !replace_existing && (results_path.exists() || decision_path.exists()) {
        bail!("immutable benchmark output already exists; refusing to overwrite it");
    }

    let results_bytes = serde_json::to_string_pretty(result)? + "\n";
    let results_temporary = write_secure_temporary(
        results_parent,
        ".benchmark-results-",
        results_bytes.as_bytes(),
    )?;
    let decision_temporary = match write_secure_temporary(
        results_parent,
        ".benchmark-decision-",
        decision.as_bytes(),
    ) {
        Ok(path) => path,
        Err(error) => {
            let _ = fs::remove_file(results_temporary);
            return Err(error);
        }
    };
    let nonce = format!("{}-{}", std::process::id(), now_ms());
    let results_backup = results_parent.join(format!(".benchmark-results-{nonce}.backup"));
    let decision_backup = results_parent.join(format!(".benchmark-decision-{nonce}.backup"));
    let had_results = results_path.exists();
    let had_decision = decision_path.exists();
    let transaction = BenchmarkPairTransaction {
        schema_version: 1,
        results_file: results_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("benchmark results filename is not UTF-8"))?
            .to_string(),
        decision_file: decision_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("benchmark decision filename is not UTF-8"))?
            .to_string(),
        results_temporary: results_temporary
            .file_name()
            .and_then(|name| name.to_str())
            .expect("temporary benchmark filename is UTF-8")
            .to_string(),
        decision_temporary: decision_temporary
            .file_name()
            .and_then(|name| name.to_str())
            .expect("temporary benchmark filename is UTF-8")
            .to_string(),
        results_backup: results_backup
            .file_name()
            .and_then(|name| name.to_str())
            .expect("benchmark backup filename is UTF-8")
            .to_string(),
        decision_backup: decision_backup
            .file_name()
            .and_then(|name| name.to_str())
            .expect("benchmark backup filename is UTF-8")
            .to_string(),
        had_results,
        had_decision,
    };
    if let Err(error) = write_transaction(results_parent, &transaction) {
        let _ = fs::remove_file(&results_temporary);
        let _ = fs::remove_file(&decision_temporary);
        return Err(error);
    }
    let replacement = (|| -> Result<()> {
        if !replace_existing {
            fs::hard_link(&results_temporary, results_path)?;
            sync_benchmark_directory(results_parent)?;
            fs::hard_link(&decision_temporary, decision_path)?;
            sync_benchmark_directory(results_parent)?;
            fs::remove_file(&results_temporary)?;
            fs::remove_file(&decision_temporary)?;
            fs::remove_file(results_parent.join(".benchmark-pair.transaction.json"))?;
            sync_benchmark_directory(results_parent)?;
            return Ok(());
        }
        if had_results {
            fs::rename(results_path, &results_backup)?;
            sync_benchmark_directory(results_parent)?;
        }
        if had_decision {
            fs::rename(decision_path, &decision_backup)?;
            sync_benchmark_directory(results_parent)?;
        }
        fs::rename(&results_temporary, results_path)?;
        sync_benchmark_directory(results_parent)?;
        fs::rename(&decision_temporary, decision_path)?;
        sync_benchmark_directory(results_parent)?;
        remove_if_present(&results_backup)?;
        remove_if_present(&decision_backup)?;
        fs::remove_file(results_parent.join(".benchmark-pair.transaction.json"))?;
        sync_benchmark_directory(results_parent)?;
        Ok(())
    })();
    if let Err(error) = replacement {
        return Err(replacement_error(
            error,
            recover_benchmark_pair(results_parent, results_path, decision_path),
        ));
    }
    Ok(())
}
