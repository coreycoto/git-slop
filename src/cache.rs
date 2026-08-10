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

pub fn status(state_root: &Path) -> Result<Value> {
    let path = database_path(state_root);
    if !path.exists() {
        return Ok(
            json!({"schema_version": 1, "command": "cache status", "status": "absent", "entries": 0, "payload_bytes": 0, "database_bytes": 0}),
        );
    }
    let connection = configured_connection(&path)?;
    let table_exists: Option<String> = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='token_cache'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if table_exists.is_none() {
        anyhow::bail!(
            "packed cache is missing token_cache table: {}",
            path.display()
        );
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
        "allocated_bytes": database_bytes.saturating_add(wal_bytes).saturating_add(shm_bytes),
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
