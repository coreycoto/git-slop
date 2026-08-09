use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tiktoken_rs::{
    CoreBPE, cl100k_base, o200k_base, o200k_harmony, p50k_base, p50k_edit, r50k_base,
};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use crate::config;
use crate::error::{ClassifiedError, ErrorKind};
use crate::estimate;
use crate::git;
use crate::health;
use crate::history;
use crate::inventory;
use crate::model::{Analysis, FileAnalysis, FindResult, ScopeIdentity};
use crate::overlays;
use crate::report;
use crate::scoring;

static CAMEL_CASE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").expect("valid camel-case regex"));
static ACRONYM_BOUNDARY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([A-Z]+)([A-Z][a-z])").expect("valid acronym regex"));
static NUMBER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d+(?:\.\d+)?\b").expect("valid number regex"));
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
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS token_cache (
               cache_key TEXT PRIMARY KEY,
               payload BLOB NOT NULL,
               payload_bytes INTEGER NOT NULL,
               accessed_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS token_cache_accessed ON token_cache(accessed_at, cache_key);",
        )?;
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
            "UPDATE token_cache SET accessed_at = unixepoch() WHERE cache_key = ?1",
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

fn replace_quoted_strings(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut previous = None;
    while let Some(character) = chars.next() {
        if !matches!(character, '\'' | '"' | '`') {
            result.push(character);
            previous = Some(character);
            continue;
        }
        if character == '\''
            && previous.is_some_and(char::is_alphanumeric)
            && chars.peek().is_some_and(|next| next.is_alphanumeric())
        {
            result.push(character);
            previous = Some(character);
            continue;
        }
        result.push_str(" str ");
        let quote = character;
        let mut escaped = false;
        for next in chars.by_ref() {
            if escaped {
                escaped = false;
                continue;
            }
            if next == '\\' {
                escaped = true;
            } else if next == quote {
                break;
            }
        }
        previous = Some(' ');
    }
    result
}

fn structural_mode(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("md" | "mdx") => "markdown",
        Some("txt") => "prose",
        Some("sql") => "sql",
        Some("html" | "htm" | "xml" | "svg") => "markup",
        _ => "code",
    }
}

fn structural_categories(mode: &str, text: &str) -> Value {
    match mode {
        "markdown" => {
            let mut fenced = false;
            let mut prose_lines = 0usize;
            let mut fenced_code_lines = 0usize;
            for line in text.lines() {
                if line.trim_start().starts_with("```") {
                    fenced = !fenced;
                } else if fenced {
                    fenced_code_lines += 1;
                } else {
                    prose_lines += 1;
                }
            }
            json!({"mode": mode, "prose_lines": prose_lines, "fenced_code_lines": fenced_code_lines})
        }
        "sql" => {
            json!({"mode": mode, "query_lines": text.lines().count(), "string_literals_normalized": true})
        }
        "markup" => {
            json!({"mode": mode, "markup_lines": text.lines().count(), "tag_and_text_categories": true})
        }
        "prose" => json!({"mode": mode, "prose_lines": text.lines().count()}),
        _ => {
            json!({"mode": "code", "code_lines": text.lines().count(), "string_literals_normalized": true})
        }
    }
}

fn structural_content_tokens(mode: &str, text: &str) -> Vec<String> {
    let normalized: String = text.nfkc().collect();
    let normalized = normalized.replace(['\u{2018}', '\u{2019}'], "'");
    let normalized = ACRONYM_BOUNDARY_RE.replace_all(&normalized, "$1 $2");
    let normalized = CAMEL_CASE_RE.replace_all(&normalized, "$1 $2");
    let normalized = normalized.replace(['-', '/'], " ");
    let normalized = if matches!(mode, "prose" | "markdown") {
        normalized
    } else {
        replace_quoted_strings(&normalized)
    };
    let normalized = NUMBER_RE.replace_all(&normalized, " 0 ");
    let lower = normalized.to_lowercase();
    lower
        .unicode_words()
        .flat_map(|word| word.split('_'))
        .map(|word| {
            word.split_once('\'')
                .filter(|(prefix, suffix)| prefix.chars().count() == 1 && !suffix.is_empty())
                .map_or(word, |(_, suffix)| suffix)
        })
        .filter(|item| item.chars().count() > 1)
        .map(ToOwned::to_owned)
        .collect()
}

fn structural_path_tokens(path: &str) -> Vec<String> {
    path.replace(['-', '_', '.'], "/")
        .to_ascii_lowercase()
        .split('/')
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
fn structural_tokens(path: &str, text: &str) -> Vec<String> {
    let mut tokens = structural_content_tokens(structural_mode(path), text);
    tokens.extend(structural_path_tokens(path));
    tokens
}

fn content_fingerprint(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn top_terms(tokens: &[String], limit: usize) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for token in tokens {
        *counts.entry(token).or_default() += 1;
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(term, _)| term.to_string())
        .collect()
}

fn has_inline_tests(language: &str, text: &str) -> bool {
    match language {
        "Rust" => text.contains("#[cfg(test)]") || text.contains("#[test]"),
        "Go" => text.contains("func Test") || text.contains("func Benchmark"),
        "Python" => text.contains("def test_") || text.contains("class Test"),
        "JavaScript" | "JSX" | "TypeScript" | "TSX" => {
            text.contains("describe(") || text.contains("test(") || text.contains("it(")
        }
        "Swift" => text.contains("XCTestCase") || text.contains("@Test"),
        _ => false,
    }
}

fn configured_context_encoder(config: &Value) -> Result<CoreBPE> {
    let tokenizer_name = match config.pointer("/tokenization/context_tokenizer_name") {
        Some(Value::String(name)) if !name.trim().is_empty() => name.as_str(),
        Some(Value::String(_)) => {
            bail!("tokenization.context_tokenizer_name must not be empty")
        }
        Some(_) => bail!("tokenization.context_tokenizer_name must be a string"),
        None => "cl100k_base",
    };
    let encoder = match tokenizer_name {
        "cl100k_base" => cl100k_base(),
        "o200k_base" => o200k_base(),
        "o200k_harmony" => o200k_harmony(),
        "p50k_base" => p50k_base(),
        "p50k_edit" => p50k_edit(),
        "r50k_base" => r50k_base(),
        unsupported => {
            bail!(
                "unsupported tokenization.context_tokenizer_name {unsupported:?}; \
                 supported encodings: cl100k_base, o200k_base, o200k_harmony, \
                 p50k_base, p50k_edit, r50k_base"
            )
        }
    };
    encoder.with_context(|| format!("failed to initialize {tokenizer_name} tokenizer"))
}

fn action_queue(files: &[FileAnalysis]) -> Vec<Value> {
    let mut files: Vec<&FileAnalysis> = files.iter().collect();
    files.sort_by(|left, right| {
        right
            .slop_score
            .total_cmp(&left.slop_score)
            .then_with(|| right.tokens.cmp(&left.tokens))
            .then_with(|| left.path.cmp(&right.path))
    });
    files
        .into_iter()
        .filter(|file| {
            !file.reason_codes.is_empty()
                || matches!(file.context_band.as_str(), "warning" | "critical")
                || matches!(file.slop_band.as_str(), "high" | "critical")
        })
        .map(|file| {
            let non_context_reasons = file.reason_codes.iter().any(|reason| {
                !matches!(reason.as_str(), "critical_token_cost" | "high_token_cost")
            });
            json!({
                "path": file.path,
                "slop_score": file.slop_score,
                "slop_band": file.slop_band,
                "context_band": file.context_band,
                "tokens": file.tokens,
                "age_days": file.age_days,
                "revisions_window": file.revisions_window,
                "churn_pressure": file.churn_pressure,
                "reason_codes": file.reason_codes,
                "is_pure_context_hotspot": !file.reason_codes.is_empty() && !non_context_reasons
            })
        })
        .collect()
}

pub fn run_find() -> Result<FindResult> {
    let repo_root = git::resolve_repo_root()?;
    run_find_in(&repo_root)
}

pub fn run_find_in(repo_root: &Path) -> Result<FindResult> {
    run_find_scoped(repo_root, false, None, false)
}

pub fn run_find_in_with_options(repo_root: &Path, allow_shallow: bool) -> Result<FindResult> {
    run_find_scoped(repo_root, allow_shallow, None, false)
}

#[derive(Debug, Clone, Default)]
pub struct FindOptions {
    pub allow_shallow: bool,
    pub scope: Option<String>,
    pub progress: bool,
    pub allow_empty_scope: bool,
    pub state_dir: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub no_cache: bool,
    pub allow_degraded: bool,
    pub report_profile: String,
    pub compression: String,
}

fn normalize_scope(value: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = value.map(str::trim) else {
        return Ok(None);
    };
    if raw.is_empty() || raw == "." {
        return Ok(None);
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        bail!("--scope must be repo-relative, received {raw:?}");
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| anyhow::anyhow!("--scope must be valid UTF-8"))?,
            ),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("--scope must not escape the repository, received {raw:?}");
            }
        }
    }
    let normalized = parts.join("/");
    Ok((!normalized.is_empty()).then_some(normalized))
}

fn selected_path_digest(paths: &[String]) -> String {
    let mut digest = Sha256::new();
    for path in paths {
        digest.update(path.as_bytes());
        digest.update([0]);
    }
    hex::encode(digest.finalize())
}

fn measure_rss_checkpoint(
    checkpoint: &'static str,
    memory_budget_bytes: u128,
    allow_degraded: bool,
    peak_rss_bytes: &mut Option<u64>,
    exceeded_checkpoints: &mut Vec<&'static str>,
) -> Result<()> {
    let Some(rss_bytes) = estimate::current_rss_bytes() else {
        return Ok(());
    };
    *peak_rss_bytes = Some(peak_rss_bytes.unwrap_or_default().max(rss_bytes));
    if u128::from(rss_bytes) <= memory_budget_bytes {
        return Ok(());
    }
    exceeded_checkpoints.push(checkpoint);
    if allow_degraded {
        return Ok(());
    }
    Err(ClassifiedError::new(
        ErrorKind::ResourceLimit,
        "measured_memory_budget_exceeded",
        format!(
            "analysis stopped at {checkpoint}: measured RSS {} MiB exceeds resources.memory_budget_mb={}; narrow --scope, use --allow-degraded, or raise the explicit budget",
            rss_bytes.div_ceil(1024 * 1024),
            memory_budget_bytes / 1024 / 1024
        ),
    )
    .at("/resources/memory_budget_mb")
    .into())
}

fn selected_content_digest(repo_root: &Path, paths: &[String]) -> Result<String> {
    let mut digest = Sha256::new();
    for path in paths {
        digest.update(path.as_bytes());
        digest.update([0]);
        let absolute = repo_root.join(path);
        let metadata = fs::symlink_metadata(&absolute)
            .with_context(|| format!("selected tracked path changed or disappeared: {path}"))?;
        let bytes = if metadata.file_type().is_symlink() {
            fs::read_link(&absolute)
                .with_context(|| format!("selected tracked link changed or disappeared: {path}"))?
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        } else if metadata.is_dir() {
            b"<gitlink>".to_vec()
        } else {
            fs::read(&absolute)
                .with_context(|| format!("selected tracked path changed or disappeared: {path}"))?
        };
        digest.update(bytes);
        digest.update([0]);
    }
    Ok(hex::encode(digest.finalize()))
}

pub fn run_find_scoped(
    repo_root: &Path,
    allow_shallow: bool,
    scope: Option<&str>,
    progress: bool,
) -> Result<FindResult> {
    run_find_with_options(
        repo_root,
        &FindOptions {
            allow_shallow,
            scope: scope.map(ToOwned::to_owned),
            progress,
            allow_empty_scope: false,
            ..FindOptions::default()
        },
    )
}

pub fn run_find_with_options(repo_root: &Path, options: &FindOptions) -> Result<FindResult> {
    let allow_shallow = options.allow_shallow;
    let scope = options.scope.as_deref();
    let progress = options.progress;
    let allow_empty_scope = options.allow_empty_scope;
    let resolve_root = |value: Option<&Path>, fallback: PathBuf| {
        value.map_or(fallback, |path| {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                repo_root.join(path)
            }
        })
    };
    let state_root = resolve_root(options.state_dir.as_deref(), config::slop_dir(repo_root));
    let output_root = resolve_root(options.output_dir.as_deref(), config::slop_dir(repo_root));
    let started = Instant::now();
    let phase = |name: &str| {
        if progress {
            eprintln!("git-slop: {name} ({:.1}s)", started.elapsed().as_secs_f64());
        }
    };
    phase("preflight");
    let _scan_lock = config::acquire_scan_lock(repo_root)?;
    let loaded_config = config::load(repo_root).map_err(|error| {
        ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_configuration",
            format!("{error:#}"),
        )
        .at("/.slop/config.yaml")
    })?;
    let mut repo = git::repo_metadata(repo_root)?;
    let runtime_exclusions = [
        state_root.join("cache"),
        output_root.join("latest"),
        output_root.join("runs"),
    ]
    .into_iter()
    .filter_map(|path| {
        path.strip_prefix(repo_root)
            .ok()
            .map(|value| value.to_string_lossy().replace('\\', "/"))
    })
    .collect::<Vec<_>>();
    let starting_worktree = git::worktree_state_excluding(repo_root, &runtime_exclusions)?;
    repo.worktree_clean = starting_worktree.clean;
    repo.staged_change_count = starting_worktree.staged_change_count;
    repo.modified_tracked_file_count = starting_worktree.modified_tracked_file_count;
    repo.untracked_file_count = starting_worktree.untracked_file_count;
    repo.worktree_state_digest = starting_worktree.digest;
    if repo.is_shallow && !allow_shallow {
        bail!(
            "repository history is shallow; rerun with git slop find --allow-shallow to acknowledge incomplete history"
        );
    }
    let all_tracked_paths = git::list_tracked_files(repo_root)?;
    let scope = normalize_scope(scope)?;
    if let Some(scope) = scope.as_deref() {
        if fs::symlink_metadata(repo_root.join(scope)).is_err() {
            bail!("--scope does not exist in the repository: {scope}");
        }
    }
    let mut tracked_paths = all_tracked_paths
        .iter()
        .filter(|path| {
            scope
                .as_deref()
                .is_none_or(|scope| *path == scope || path.starts_with(&format!("{scope}/")))
        })
        .cloned()
        .collect::<Vec<_>>();
    if tracked_paths.is_empty() && !allow_empty_scope {
        bail!(
            "{} selected no tracked paths; pass --allow-empty-scope only when an empty report is intentional",
            scope.as_deref().map_or_else(
                || "repository".to_string(),
                |scope| format!("--scope {scope:?}")
            )
        );
    }
    let original_selected_path_count = tracked_paths.len();
    let initial_estimate = estimate::build(repo_root, &tracked_paths, &loaded_config);
    if initial_estimate.estimated_peak_memory_bytes > initial_estimate.memory_budget_bytes
        && options.allow_degraded
    {
        let mut low = 0usize;
        let mut high = tracked_paths.len();
        while low < high {
            let middle = (low + high).div_ceil(2);
            let candidate = estimate::build(repo_root, &tracked_paths[..middle], &loaded_config);
            if candidate.estimated_peak_memory_bytes <= candidate.memory_budget_bytes {
                low = middle;
            } else {
                high = middle.saturating_sub(1);
            }
        }
        tracked_paths.truncate(low);
    }
    let scope_identity = ScopeIdentity {
        mode: if scope.is_some() {
            "scoped"
        } else {
            "repository"
        }
        .to_string(),
        path: scope.clone(),
        selected_path_count: tracked_paths.len(),
        selected_path_digest: selected_path_digest(&tracked_paths),
    };
    let starting_content_digest = selected_content_digest(repo_root, &tracked_paths)?;
    let estimate = estimate::build(repo_root, &tracked_paths, &loaded_config);
    if estimate.estimated_peak_memory_bytes > estimate.memory_budget_bytes {
        return Err(ClassifiedError::new(
            ErrorKind::ResourceLimit,
            "estimated_memory_budget_exceeded",
            format!(
                "analysis bounded before inventory: estimated {} MiB exceeds resources.memory_budget_mb={}; narrow --scope, use --allow-degraded, or raise the explicit budget",
                estimate.estimated_peak_memory_bytes.div_ceil(1024 * 1024),
                estimate.memory_budget_bytes / 1024 / 1024
            ),
        )
        .at("/resources/memory_budget_mb")
        .into());
    }
    let mut measured_peak_rss_bytes = None;
    let mut memory_budget_exceeded_checkpoints = Vec::new();
    measure_rss_checkpoint(
        "pre_inventory",
        estimate.memory_budget_bytes,
        options.allow_degraded,
        &mut measured_peak_rss_bytes,
        &mut memory_budget_exceeded_checkpoints,
    )?;
    let (inventory_files, skipped) = inventory::build(repo_root, &tracked_paths, &loaded_config)?;
    phase("inventory");
    measure_rss_checkpoint(
        "post_inventory",
        estimate.memory_budget_bytes,
        options.allow_degraded,
        &mut measured_peak_rss_bytes,
        &mut memory_budget_exceeded_checkpoints,
    )?;
    let encoder = configured_context_encoder(&loaded_config).map_err(|error| {
        ClassifiedError::new(
            ErrorKind::Contract,
            "unsupported_tokenizer",
            format!("{error:#}"),
        )
        .at("/tokenization/context_tokenizer_name")
    })?;
    let mut token_counts = BTreeMap::new();
    let mut line_counts = BTreeMap::new();
    let mut token_data = HashMap::new();
    let tokenizer = config::pointer_str(&loaded_config, "/tokenization/context_tokenizer_name")
        .unwrap_or("cl100k_base")
        .to_string();
    let large_file_bytes =
        config::pointer_u64(&loaded_config, "/resources/large_file_bytes", 2_097_152) as usize;
    let cache_path = state_root.join("cache").join("token-v4.sqlite3");
    let mut cache_cleanup_warnings = Vec::new();
    if !options.no_cache {
        for version in ["token-v1", "token-v2", "token-v3"] {
            let legacy_cache = state_root.join("cache").join(version);
            if legacy_cache.exists() {
                if let Err(error) = fs::remove_dir_all(&legacy_cache) {
                    cache_cleanup_warnings.push(format!(
                        "failed to remove legacy cache {}: {error}",
                        legacy_cache.display()
                    ));
                }
            }
        }
    }
    let cache = if options.no_cache {
        None
    } else {
        Some(TokenCache::open(&cache_path)?)
    };
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;
    let mut structurally_skipped_large_files = 0usize;
    for file in &inventory_files {
        if file.analysis_status != "analyzed" || file.bytes > large_file_bytes {
            structurally_skipped_large_files += 1;
        }
        if file.analysis_status != "analyzed" {
            token_counts.insert(file.path.clone(), 0);
            line_counts.insert(file.path.clone(), 0);
            token_data.insert(
                file.path.clone(),
                (0, Vec::new(), Vec::new(), String::new()),
            );
            continue;
        }
        let mode = structural_mode(&file.path);
        let cache_key = token_cache_key(&file.text, &tokenizer, large_file_bytes, mode);
        let cached_value = cache
            .as_ref()
            .map(|cache| cache.get(&cache_key))
            .transpose()?
            .flatten();
        let cached = if let Some(cached) = cached_value {
            cache_hits += 1;
            cached
        } else {
            cache_misses += 1;
            let cached = CachedTokenData {
                token_count: encoder.encode_ordinary(&file.text).len(),
                structural_tokens: if file.bytes > large_file_bytes {
                    Vec::new()
                } else {
                    structural_content_tokens(mode, &file.text)
                },
                content_fingerprint: content_fingerprint(&file.text),
            };
            if let Some(cache) = &cache {
                cache.put(&cache_key, &cached)?;
            }
            cached
        };
        let count = cached.token_count;
        let mut structural = cached.structural_tokens;
        if file.bytes <= large_file_bytes {
            structural.extend(structural_path_tokens(&file.path));
        }
        let top_term_limit =
            config::pointer_u64(&loaded_config, "/semantic_drift/top_term_limit", 25) as usize;
        let fingerprint = cached.content_fingerprint;
        token_counts.insert(file.path.clone(), count);
        line_counts.insert(file.path.clone(), file.lines);
        token_data.insert(
            file.path.clone(),
            (
                structural.len(),
                top_terms(&structural, top_term_limit),
                structural,
                fingerprint,
            ),
        );
    }
    let cache_stats = if let Some(cache) = &cache {
        cache.enforce_limits(
            config::pointer_u64(&loaded_config, "/resources/cache_max_entries", 10_000) as usize,
            config::pointer_u64(&loaded_config, "/resources/cache_max_bytes", 536_870_912),
        )?
    } else {
        CacheStats::default()
    };
    phase("tokenization");
    measure_rss_checkpoint(
        "post_tokenization",
        estimate.memory_budget_bytes,
        options.allow_degraded,
        &mut measured_peak_rss_bytes,
        &mut memory_budget_exceeded_checkpoints,
    )?;
    repo.analyzed_content_digest = Some(starting_content_digest.clone());
    let analyzed_paths: Vec<String> = inventory_files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let now = Utc::now();
    let (history_by_path, commits, history_diagnostics) = history::analyze_history(
        repo_root,
        &analyzed_paths,
        &token_counts,
        &line_counts,
        &loaded_config,
        now,
    )?;
    phase("history");
    measure_rss_checkpoint(
        "post_history",
        estimate.memory_budget_bytes,
        options.allow_degraded,
        &mut measured_peak_rss_bytes,
        &mut memory_budget_exceeded_checkpoints,
    )?;
    let mut files = Vec::with_capacity(inventory_files.len());
    for file in inventory_files {
        let tokens = token_counts.get(&file.path).copied().unwrap_or_default();
        let inline_tests = has_inline_tests(&file.language, &file.text);
        let (structural_token_count, top_structural_terms, structural_tokens, content_fingerprint) =
            token_data
                .remove(&file.path)
                .unwrap_or_else(|| (0, Vec::new(), Vec::new(), String::new()));
        let history = history_by_path.get(&file.path).cloned().unwrap_or_default();
        let categories = structural_categories(structural_mode(&file.path), &file.text);
        files.push(FileAnalysis {
            path: file.path,
            bytes: file.bytes,
            lines: file.lines,
            blank_lines: file.blank_lines,
            code_lines: file.code_lines,
            comment_lines: file.comment_lines,
            language: file.language,
            profile: file.profile,
            classification: file.classification,
            analysis_status: file.analysis_status,
            skipped_reason: file.skipped_reason,
            symlink_metadata: file.symlink_metadata,
            has_inline_tests: inline_tests,
            tokens,
            context_band: scoring::context_band_for_tokens(tokens, &loaded_config),
            context_pressure: scoring::context_pressure_for_tokens(tokens, &loaded_config),
            content_fingerprint,
            structural_tokens,
            structural_token_count,
            top_structural_terms,
            structural_categories: categories,
            age_days: history.age_days,
            revisions_window: history.revisions_window,
            recency_weighted_commits: history.recency_weighted_commits,
            added_window: history.added_window,
            deleted_window: history.deleted_window,
            churn_lines_window: history.line_churn_window,
            line_churn_window: history.line_churn_window,
            token_churn_window: history.token_churn_window,
            relative_churn_window: history.relative_churn_window,
            late_churn_spike: history.late_churn_spike,
            author_count_window: history.author_count_window,
            author_entropy: history.author_entropy,
            top_author_share: history.top_author_share,
            days_since_non_bot_edit: history.days_since_non_bot_edit,
            recent_maintainer_diversity: history.recent_maintainer_diversity,
            age_pressure: 0.0,
            revision_norm: 0.0,
            relative_churn_norm: 0.0,
            churn_pressure: 0.0,
            slop_score: 0.0,
            slop_band: "low".to_string(),
            reason_codes: Vec::new(),
            costs: json!({}),
            overlays: json!({}),
        });
    }
    scoring::apply_scoring(&mut files, &loaded_config);
    let organization = overlays::analyze(&mut files, &commits, &loaded_config)?;
    phase("relationships");
    measure_rss_checkpoint(
        "post_relationships",
        estimate.memory_budget_bytes,
        options.allow_degraded,
        &mut measured_peak_rss_bytes,
        &mut memory_budget_exceeded_checkpoints,
    )?;
    let folders = scoring::build_folder_analyses(&files, &loaded_config);
    let queue = action_queue(&files);
    let generated_at = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    let analyzed_revision_at = repo.head_commit_timestamp.clone();
    let ending_worktree = git::worktree_state_excluding(repo_root, &runtime_exclusions)?;
    if ending_worktree.digest != repo.worktree_state_digest {
        bail!("repository changed during analysis; no mixed-snapshot report was published");
    }
    if selected_content_digest(repo_root, &tracked_paths)? != starting_content_digest {
        bail!(
            "selected file content changed during analysis; no mixed-snapshot report was published"
        );
    }
    let analysis = Analysis {
        output_root,
        report_profile: if options.report_profile.is_empty() {
            "standard".to_string()
        } else {
            options.report_profile.clone()
        },
        compression: if options.compression.is_empty() {
            "none".to_string()
        } else {
            options.compression.clone()
        },
        repo,
        config: loaded_config,
        generated_at,
        analyzed_revision_at,
        skipped,
        tracked_file_count: all_tracked_paths.len(),
        scope: scope_identity,
        files,
        folders,
        organization,
        action_queue: queue,
        diagnostics: json!({
            "analysis_elapsed_ms_before_report": started.elapsed().as_millis(),
            "estimate": estimate,
            "measured_peak_rss_bytes": measured_peak_rss_bytes,
            "memory_budget_exceeded_checkpoints": memory_budget_exceeded_checkpoints,
            "memory_measurement_status": if measured_peak_rss_bytes.is_some() { "measured" } else { "unsupported" },
            "cache_hits": cache_hits,
            "cache_misses": cache_misses,
            "cache_entries": cache_stats.entries,
            "cache_bytes": cache_stats.bytes,
            "cache_failed_evictions": cache_stats.failed_evictions,
            "cache_cleanup_warnings": cache_cleanup_warnings,
            "cache_status": if options.no_cache { "disabled" } else { "enabled" },
            "structurally_skipped_large_files": structurally_skipped_large_files,
            "analysis_status": if tracked_paths.len() < original_selected_path_count || !memory_budget_exceeded_checkpoints.is_empty() { "degraded_resource_budget" } else if structurally_skipped_large_files > 0 { "degraded_large_files" } else { "complete" },
            "resource_mode": if tracked_paths.len() < original_selected_path_count { "degraded_path_prefix" } else if !memory_budget_exceeded_checkpoints.is_empty() { "degraded_measured_rss" } else { "complete" },
            "original_selected_path_count": original_selected_path_count,
            "degraded_omitted_path_count": original_selected_path_count.saturating_sub(tracked_paths.len()),
            "history": history_diagnostics,
            "scope": scope
        }),
    };
    let rollup = health::build_health_rollup(&analysis);
    let result = report::write_report_bundle(&analysis, &rollup)?;
    phase("report writing");
    if result.report.get("schema_version").and_then(Value::as_u64) != Some(5) {
        bail!("internal error: report writer did not produce schema 5");
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tiktoken_rs::{cl100k_base, r50k_base};

    use super::{
        action_queue, configured_context_encoder, replace_quoted_strings, structural_tokens,
    };
    use crate::model::FileAnalysis;
    use crate::scoring;

    fn file(path: &str, relative_churn: f64) -> FileAnalysis {
        FileAnalysis {
            path: path.to_string(),
            bytes: 400,
            lines: 100,
            blank_lines: 0,
            code_lines: 100,
            comment_lines: 0,
            language: "Rust".to_string(),
            profile: "agent_context".to_string(),
            classification: "source".to_string(),
            analysis_status: "analyzed".to_string(),
            skipped_reason: None,
            symlink_metadata: None,
            has_inline_tests: false,
            tokens: 100,
            context_band: "compact".to_string(),
            context_pressure: 0.0,
            content_fingerprint: String::new(),
            structural_tokens: Vec::new(),
            structural_token_count: 0,
            top_structural_terms: Vec::new(),
            structural_categories: json!({"mode": "code"}),
            age_days: 0,
            revisions_window: 1,
            recency_weighted_commits: 0.0,
            added_window: 0,
            deleted_window: 0,
            churn_lines_window: 0,
            line_churn_window: 0,
            token_churn_window: 0,
            relative_churn_window: relative_churn,
            late_churn_spike: 0.0,
            author_count_window: 0,
            author_entropy: 0.0,
            top_author_share: 0.0,
            days_since_non_bot_edit: None,
            recent_maintainer_diversity: 0,
            age_pressure: 0.0,
            revision_norm: 0.0,
            relative_churn_norm: 0.0,
            churn_pressure: 0.0,
            slop_score: 0.0,
            slop_band: String::new(),
            reason_codes: Vec::new(),
            costs: json!({}),
            overlays: json!({}),
        }
    }

    #[test]
    fn structural_normalization_is_deterministic() {
        let tokens = structural_tokens(
            "src/my_file.rs",
            "let camelCase = \"secret 123\"; // hello-world",
        );
        assert!(tokens.contains(&"camel".to_string()));
        assert!(tokens.contains(&"case".to_string()));
        assert!(tokens.contains(&"str".to_string()));
        assert!(tokens.contains(&"my".to_string()));
        assert_eq!(
            replace_quoted_strings("'one' \"two\" `three`"),
            " str   str   str "
        );
    }

    #[test]
    fn structural_normalization_preserves_unicode_and_apostrophe_words() {
        let tokens = structural_tokens("docs/café.md", "L’équipe can’t rename HTTPServer_value");
        assert!(tokens.contains(&"équipe".to_string()));
        assert!(tokens.contains(&"can't".to_string()));
        assert!(tokens.contains(&"http".to_string()));
        assert!(tokens.contains(&"server".to_string()));
        assert!(tokens.contains(&"value".to_string()));
        assert!(tokens.iter().any(|token| token.contains("café")));
    }

    #[test]
    fn configured_tokenizer_is_used_exactly_and_unknown_names_fail_closed() {
        let text = "お誕生日おめでとう";
        let configured = configured_context_encoder(&json!({"tokenization": {
            "context_tokenizer_name": "r50k_base"
        }}))
        .unwrap();
        assert_eq!(
            configured.encode_ordinary(text).len(),
            r50k_base().unwrap().encode_ordinary(text).len()
        );
        assert_ne!(
            configured.encode_ordinary(text).len(),
            cl100k_base().unwrap().encode_ordinary(text).len()
        );

        let result = configured_context_encoder(&json!({"tokenization": {
            "context_tokenizer_name": "not-a-real-encoding"
        }}));
        let error = match result {
            Ok(_) => panic!("unsupported tokenizer must fail closed"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("unsupported tokenization.context_tokenizer_name")
        );
    }

    #[test]
    fn action_queue_prioritizes_line_relative_churn_signal() {
        let mut files = vec![file("src/quiet.rs", 0.1), file("src/volatile.rs", 2.0)];
        scoring::apply_scoring(&mut files, &json!({}));
        let queue = action_queue(&files);

        assert_eq!(queue[0]["path"], "src/volatile.rs");
        assert_eq!(queue[0]["reason_codes"][1], "high_relative_churn");
        assert_eq!(queue[0]["is_pure_context_hotspot"], false);
    }
}
