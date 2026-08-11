#[derive(Debug, Serialize, Deserialize)]
struct CachedTokenData {
    token_count: usize,
    structural_tokens: Vec<String>,
    content_fingerprint: String,
}

fn token_cache_key(text: &str, tokenizer: &str, large_file_bytes: usize, mode: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"git-slop-token-cache-v3\0");
    digest.update(tokenizer.as_bytes());
    digest.update([0]);
    digest.update(large_file_bytes.to_le_bytes());
    digest.update(mode.as_bytes());
    digest.update([0]);
    digest.update(text.as_bytes());
    hex::encode(digest.finalize())
}

struct TokenCache {
    connection: Connection,
}

#[derive(Default)]
struct CacheStats {
    entries: usize,
    bytes: u64,
    failed_evictions: usize,
}

impl TokenCache {
    fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)
            .with_context(|| format!("failed to open packed cache {}", path.display()))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let schema_version: u32 =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        match schema_version {
            0 => connection.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 CREATE TABLE token_cache (
                   cache_key TEXT PRIMARY KEY,
                   payload BLOB NOT NULL,
                   payload_bytes INTEGER NOT NULL,
                   accessed_at INTEGER NOT NULL
                 );
                 CREATE INDEX token_cache_accessed ON token_cache(accessed_at, cache_key);
                 PRAGMA user_version=1;",
            )?,
            1 => connection.execute_batch("PRAGMA synchronous=NORMAL;")?,
            version => bail!("unsupported packed cache schema version {version}"),
        }
        let integrity: String =
            connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if integrity != "ok" {
            bail!("packed cache integrity check failed: {integrity}");
        }
        Ok(Self { connection })
    }

    fn get(&self, key: &str) -> Result<Option<CachedTokenData>> {
        let payload: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT payload FROM token_cache WHERE cache_key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        self.connection.execute(
            "UPDATE token_cache SET accessed_at = unixepoch() WHERE cache_key = ?1 AND accessed_at < unixepoch() - 3600",
            [key],
        )?;
        Ok(serde_json::from_slice(&payload).ok())
    }

    fn put(&self, key: &str, value: &CachedTokenData) -> Result<()> {
        let payload = serde_json::to_vec(value)?;
        let payload_bytes = payload.len() as u64;
        self.connection.execute(
            "INSERT INTO token_cache(cache_key, payload, payload_bytes, accessed_at)
             VALUES(?1, ?2, ?3, unixepoch())
             ON CONFLICT(cache_key) DO UPDATE SET
               payload = excluded.payload,
               payload_bytes = excluded.payload_bytes,
               accessed_at = excluded.accessed_at",
            params![key, payload, payload_bytes],
        )?;
        Ok(())
    }

    fn enforce_limits(&self, max_entries: usize, max_bytes: u64) -> Result<CacheStats> {
        let mut stats = self.stats()?;
        while stats.entries > max_entries || stats.bytes > max_bytes {
            let candidate: Option<(String, u64)> = self
                .connection
                .query_row(
                    "SELECT cache_key, payload_bytes FROM token_cache ORDER BY accessed_at, cache_key LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((key, bytes)) = candidate else {
                break;
            };
            match self
                .connection
                .execute("DELETE FROM token_cache WHERE cache_key = ?1", [&key])
            {
                Ok(1) => {
                    stats.entries = stats.entries.saturating_sub(1);
                    stats.bytes = stats.bytes.saturating_sub(bytes);
                }
                _ => {
                    stats.failed_evictions += 1;
                    break;
                }
            }
        }
        Ok(stats)
    }

    fn stats(&self) -> Result<CacheStats> {
        let (entries, bytes): (u64, u64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(payload_bytes), 0) FROM token_cache",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(CacheStats {
            entries: entries as usize,
            bytes,
            failed_evictions: 0,
        })
    }
}

fn quarantine_cache(path: &Path, error: &anyhow::Error) -> String {
    let message = error.root_cause().to_string();
    if message.contains("database is locked") || message.contains("database is busy") {
        return format!("packed token cache was busy; continued uncached: {message}");
    }
    if !path.exists() {
        return format!("packed token cache was unavailable; continued uncached: {message}");
    }
    let suffix = format!("corrupt-{}", Utc::now().timestamp());
    let quarantine = path.with_extension(format!("sqlite3.{suffix}"));
    match fs::rename(path, &quarantine) {
        Ok(()) => {
            for sidecar in ["-wal", "-shm"] {
                let source = PathBuf::from(format!("{}{sidecar}", path.display()));
                if source.exists() {
                    let target = PathBuf::from(format!("{}{sidecar}", quarantine.display()));
                    let _ = fs::rename(source, target);
                }
            }
            format!(
                "packed token cache failed validation and was quarantined; a fresh cache was opened when possible: {message}"
            )
        }
        Err(quarantine_error) => format!(
            "packed token cache failed and could not be quarantined ({quarantine_error}); continued uncached: {message}"
        ),
    }
}
