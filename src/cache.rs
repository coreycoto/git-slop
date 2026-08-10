use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

fn database_path(state_root: &Path) -> PathBuf {
    state_root.join("cache").join("token-v4.sqlite3")
}

fn configured_connection(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)
        .with_context(|| format!("failed to open packed cache {}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(5))?;
    Ok(connection)
}

fn sidecar_bytes(path: &Path, suffix: &str) -> u64 {
    fs::metadata(format!("{}{suffix}", path.display()))
        .map(|metadata| metadata.len())
        .unwrap_or_default()
}

fn quarantined_stats(path: &Path) -> (usize, u64) {
    let Some(parent) = path.parent() else {
        return (0, 0);
    };
    let prefix = format!(
        "{}.",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("token-v4.sqlite3")
    );
    fs::read_dir(parent)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .filter_map(|entry| fs::metadata(entry.path()).ok())
        .fold((0usize, 0u64), |(count, bytes), metadata| {
            (count + 1, bytes.saturating_add(metadata.len()))
        })
}

pub fn status(state_root: &Path) -> Result<Value> {
    let path = database_path(state_root);
    let (quarantined_files, quarantined_bytes) = quarantined_stats(&path);
    if !path.exists() {
        return Ok(
            json!({"schema_version": 1, "command": "cache status", "status": if quarantined_files > 0 { "quarantined_uncached" } else { "absent" }, "entries": 0, "payload_bytes": 0, "database_bytes": 0, "wal_bytes": 0, "shm_bytes": 0, "persistent_bytes": 0, "transient_bytes": 0, "allocated_bytes": 0, "quarantined_files": quarantined_files, "quarantined_bytes": quarantined_bytes, "repair_command": "git slop find", "cleanup_command": "git slop cache prune --max-entries 0 --max-bytes 0 --compact"}),
        );
    }
    let connection = match configured_connection(&path) {
        Ok(connection) => connection,
        Err(error) => {
            return Ok(json!({
                "schema_version": 1, "command": "cache status", "status": "unavailable",
                "entries": 0, "payload_bytes": 0,
                "database_bytes": fs::metadata(&path).map(|value| value.len()).unwrap_or_default(),
                "wal_bytes": sidecar_bytes(&path, "-wal"), "shm_bytes": sidecar_bytes(&path, "-shm"),
                "integrity": "not_checked", "diagnostic": format!("cache could not be opened: {}", error.root_cause())
            }));
        }
    };
    let table_exists: Option<String> = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='token_cache'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if table_exists.is_none() {
        return Ok(json!({
            "schema_version": 1, "command": "cache status", "status": "invalid_schema",
            "entries": 0, "payload_bytes": 0,
            "database_bytes": fs::metadata(&path).map(|value| value.len()).unwrap_or_default(),
            "wal_bytes": sidecar_bytes(&path, "-wal"), "shm_bytes": sidecar_bytes(&path, "-shm"),
            "integrity": "token_cache_table_missing"
        }));
    }
    let (entries, bytes, oldest, newest): (u64, u64, Option<i64>, Option<i64>) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(payload_bytes), 0), MIN(accessed_at), MAX(accessed_at) FROM token_cache",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let integrity: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    let page_count: u64 = connection.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let free_pages: u64 = connection.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    let page_size: u64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let database_bytes = fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    let wal_bytes = sidecar_bytes(&path, "-wal");
    let shm_bytes = sidecar_bytes(&path, "-shm");
    Ok(json!({
        "schema_version": 1, "command": "cache status", "status": if integrity == "ok" { "ready" } else { "corrupt" },
        "entries": entries, "payload_bytes": bytes,
        "database_bytes": database_bytes, "wal_bytes": wal_bytes, "shm_bytes": shm_bytes,
        "persistent_bytes": database_bytes, "transient_bytes": wal_bytes.saturating_add(shm_bytes),
        "allocated_bytes": database_bytes.saturating_add(wal_bytes).saturating_add(shm_bytes),
        "quarantined_files": quarantined_files, "quarantined_bytes": quarantined_bytes,
        "page_count": page_count, "free_pages": free_pages, "page_size": page_size,
        "reclaimable_bytes": free_pages.saturating_mul(page_size), "integrity": integrity,
        "oldest_accessed_at_unix": oldest, "newest_accessed_at_unix": newest
    }))
}

pub fn prune(
    state_root: &Path,
    max_entries: usize,
    max_bytes: u64,
    dry_run: bool,
    compact: bool,
) -> Result<Value> {
    let path = database_path(state_root);
    let before = status(state_root)?;
    if !path.exists() {
        return Ok(
            json!({"schema_version": 1, "command": "cache prune", "dry_run": dry_run, "before": before, "removed_entries": 0, "removed_payload_bytes": 0, "after": before}),
        );
    }
    let mut connection = configured_connection(&path)?;
    let candidates = {
        let mut statement = connection.prepare(
            "SELECT cache_key, payload_bytes FROM token_cache ORDER BY accessed_at, cache_key",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut entries = before["entries"].as_u64().unwrap_or_default() as usize;
    let mut bytes = before["payload_bytes"].as_u64().unwrap_or_default();
    let mut selected = Vec::new();
    for (key, candidate_bytes) in candidates {
        if entries <= max_entries && bytes <= max_bytes {
            break;
        }
        entries = entries.saturating_sub(1);
        bytes = bytes.saturating_sub(candidate_bytes);
        selected.push((key, candidate_bytes));
    }
    if !dry_run {
        let transaction = connection.transaction()?;
        for (key, _) in &selected {
            transaction.execute("DELETE FROM token_cache WHERE cache_key = ?1", [key])?;
        }
        transaction.commit()?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize;")?;
        if compact {
            connection.execute_batch("VACUUM;")?;
        }
    }
    let removed_bytes = selected.iter().map(|(_, bytes)| *bytes).sum::<u64>();
    if !dry_run {
        drop(connection);
    }
    let after = if dry_run {
        json!({"entries": entries, "payload_bytes": bytes, "projected": true})
    } else {
        status(state_root)?
    };
    Ok(json!({
        "schema_version": 1, "command": "cache prune", "dry_run": dry_run,
        "limits": {"max_entries": max_entries, "max_bytes": max_bytes}, "compact": compact,
        "before": before, "removed_entries": selected.len(),
        "removed_payload_bytes": removed_bytes,
        "physical_reclaimed_bytes": before["allocated_bytes"].as_u64().unwrap_or_default().saturating_sub(after["allocated_bytes"].as_u64().unwrap_or_default()),
        "after": after
    }))
}
