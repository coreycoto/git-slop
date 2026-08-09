use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

fn database_path(state_root: &Path) -> PathBuf {
    state_root.join("cache").join("token-v4.sqlite3")
}

pub fn status(state_root: &Path) -> Result<Value> {
    let path = database_path(state_root);
    if !path.exists() {
        return Ok(
            json!({"schema_version": 1, "command": "cache status", "status": "absent", "entries": 0, "payload_bytes": 0, "database_bytes": 0}),
        );
    }
    let connection = Connection::open(&path)
        .with_context(|| format!("failed to open packed cache {}", path.display()))?;
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
    Ok(json!({
        "schema_version": 1, "command": "cache status", "status": "ready",
        "entries": entries, "payload_bytes": bytes,
        "database_bytes": fs::metadata(&path).map(|metadata| metadata.len()).unwrap_or_default(),
        "oldest_accessed_at_unix": oldest, "newest_accessed_at_unix": newest
    }))
}

pub fn prune(
    state_root: &Path,
    max_entries: usize,
    max_bytes: u64,
    dry_run: bool,
) -> Result<Value> {
    let path = database_path(state_root);
    let before = status(state_root)?;
    if !path.exists() {
        return Ok(
            json!({"schema_version": 1, "command": "cache prune", "dry_run": dry_run, "before": before, "removed_entries": 0, "removed_payload_bytes": 0, "after": before}),
        );
    }
    let mut connection = Connection::open(&path)?;
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
        connection.execute_batch("PRAGMA optimize;")?;
    }
    let removed_bytes = selected.iter().map(|(_, bytes)| *bytes).sum::<u64>();
    let after = if dry_run {
        json!({"entries": entries, "payload_bytes": bytes, "projected": true})
    } else {
        status(state_root)?
    };
    Ok(json!({
        "schema_version": 1, "command": "cache prune", "dry_run": dry_run,
        "limits": {"max_entries": max_entries, "max_bytes": max_bytes},
        "before": before, "removed_entries": selected.len(),
        "removed_payload_bytes": removed_bytes, "after": after
    }))
}
