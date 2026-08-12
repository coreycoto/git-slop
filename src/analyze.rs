use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, SecondsFormat, Utc};
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
static RUST_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe)\s+)?(?:fn|struct|enum|trait|mod|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)")
        .expect("valid Rust symbol regex")
});
static PYTHON_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?:async\s+)?(?:def|class)\s+([A-Za-z_][A-Za-z0-9_]*)")
        .expect("valid Python symbol regex")
});
static GO_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?:func(?:\s+\([^)]*\))?|type)\s+([A-Za-z_][A-Za-z0-9_]*)")
        .expect("valid Go symbol regex")
});
static JS_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function|class|interface|type)\s+([A-Za-z_$][A-Za-z0-9_$]*)")
        .expect("valid JavaScript symbol regex")
});
static MARKDOWN_HEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s{0,3}#{1,6}\s+([^#\r\n]+?)\s*#*\s*$").expect("valid Markdown heading regex")
});
include!("analyze/cache.rs");
include!("analyze/structural.rs");
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
    pub as_of: Option<DateTime<Utc>>,
    pub report_profile: String,
    pub compression: String,
}

pub(crate) fn normalize_scope(value: Option<&str>) -> Result<Option<String>> {
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
        return Err(ClassifiedError::new(
            ErrorKind::ResourceLimit,
            "degraded_memory_recovery_unavailable",
            format!(
                "analysis stopped at {checkpoint}: measured RSS {} MiB still exceeds resources.memory_budget_mb={} after deterministic degraded sampling; continuing would violate the memory contract",
                rss_bytes.div_ceil(1024 * 1024),
                memory_budget_bytes / 1024 / 1024
            ),
        )
        .at("/resources/memory_budget_mb")
        .into());
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

fn balanced_path_sample(paths: &[String], limit: usize) -> Vec<String> {
    let mut roots = BTreeMap::<&str, Vec<&String>>::new();
    for path in paths {
        roots
            .entry(path.split('/').next().unwrap_or("."))
            .or_default()
            .push(path);
    }
    let mut selected = Vec::with_capacity(limit.min(paths.len()));
    let mut offset = 0usize;
    while selected.len() < limit {
        let mut added = false;
        for values in roots.values() {
            if let Some(path) = values.get(offset) {
                selected.push((*path).clone());
                added = true;
                if selected.len() == limit {
                    break;
                }
            }
        }
        if !added {
            break;
        }
        offset += 1;
    }
    selected.sort();
    selected
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
    let _scan_lock = config::acquire_scan_lock(&state_root)?;
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
        tracked_paths = balanced_path_sample(&tracked_paths, low);
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
    let mut cache = if options.no_cache {
        None
    } else {
        match TokenCache::open(&cache_path) {
            Ok(cache) => Some(cache),
            Err(error) => {
                cache_cleanup_warnings.push(quarantine_cache(&cache_path, &error));
                TokenCache::open(&cache_path).ok()
            }
        }
    };
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;
    let mut structurally_skipped_large_files = 0usize;
    let mut intentionally_skipped_non_text_files = 0usize;
    let mut incomplete_inventory_files = 0usize;
    for file in &inventory_files {
        if file.skipped_reason.as_deref() == Some("large_file_limit") {
            structurally_skipped_large_files += 1;
        }
        if matches!(
            file.skipped_reason.as_deref(),
            Some("binary" | "gitlink" | "undecodable")
        ) {
            intentionally_skipped_non_text_files += 1;
        } else if file.analysis_status != "analyzed"
            && file.skipped_reason.as_deref() != Some("large_file_limit")
        {
            incomplete_inventory_files += 1;
        }
        if file.analysis_status != "analyzed" {
            let conservative_tokens = if file.skipped_reason.as_deref() == Some("large_file_limit")
            {
                file.bytes.div_ceil(4)
            } else {
                0
            };
            token_counts.insert(file.path.clone(), conservative_tokens);
            line_counts.insert(file.path.clone(), 0);
            token_data.insert(
                file.path.clone(),
                (
                    0,
                    Vec::new(),
                    Vec::new(),
                    format!(
                        "incomplete:{}:{}",
                        file.skipped_reason.as_deref().unwrap_or("unknown"),
                        file.bytes
                    ),
                ),
            );
            continue;
        }
        let mode = structural_mode(&file.path);
        let cache_key = token_cache_key(&file.text, &tokenizer, large_file_bytes, mode);
        let cached_value = if let Some(active_cache) = cache.as_ref() {
            match active_cache.get(&cache_key) {
                Ok(value) => value,
                Err(error) => {
                    cache_cleanup_warnings.push(quarantine_cache(&cache_path, &error));
                    cache = TokenCache::open(&cache_path).ok();
                    None
                }
            }
        } else {
            None
        };
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
            let put_error = cache
                .as_ref()
                .and_then(|active_cache| active_cache.put(&cache_key, &cached).err());
            if let Some(error) = put_error {
                cache_cleanup_warnings.push(quarantine_cache(&cache_path, &error));
                cache = TokenCache::open(&cache_path).ok();
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
                top_terms(&structural, &file.language, &file.text, top_term_limit),
                structural,
                fingerprint,
            ),
        );
    }
    let cache_stats = if let Some(cache) = &cache {
        match cache.enforce_limits(
            config::pointer_u64(&loaded_config, "/resources/cache_max_entries", 10_000) as usize,
            config::pointer_u64(&loaded_config, "/resources/cache_max_bytes", 536_870_912),
        ) {
            Ok(stats) => stats,
            Err(error) => {
                cache_cleanup_warnings.push(quarantine_cache(&cache_path, &error));
                CacheStats::default()
            }
        }
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
    let now = options.as_of.unwrap_or_else(Utc::now);
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
            profile: file.profile.clone(),
            classification: file.classification,
            generated_from: file.generated_from,
            generated_provenance: file.generated_provenance,
            analysis_status: file.analysis_status,
            skipped_reason: file.skipped_reason,
            symlink_metadata: file.symlink_metadata,
            has_inline_tests: inline_tests,
            tokens,
            context_band: scoring::context_band_for_profile(tokens, &file.profile, &loaded_config),
            context_pressure: scoring::context_pressure_for_profile(
                tokens,
                &file.profile,
                &loaded_config,
            ),
            content_fingerprint,
            content_sha256: file.content_sha256,
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
    let history_evidence_reliable = !repo.is_shallow
        && !history_diagnostics
            .get("history_cap_reached")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && ![
            "full_history_cap_status",
            "window_status_cap_status",
            "window_numstat_cap_status",
        ]
        .into_iter()
        .any(|field| history_diagnostics.get(field).and_then(Value::as_str) == Some("truncated"));
    scoring::apply_scoring_with_evidence(&mut files, &loaded_config, history_evidence_reliable);
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
    let candidates = action_queue(&files, history_evidence_reliable, &loaded_config);
    let (queue, observation_feed): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|item| {
        let classification = item
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or("other");
        let actionable = !matches!(
            classification,
            "generated" | "vendored" | "snapshot" | "fixture" | "migration_fixture"
        );
        let supported = item.get("evidence_status").and_then(Value::as_str) == Some("supported")
            || item.get("is_pure_context_hotspot").and_then(Value::as_bool) == Some(true);
        actionable
            && supported
            && matches!(
                item.get("severity").and_then(Value::as_str),
                Some("warning" | "error")
            )
    });
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
    let estimator_error_ratio = measured_peak_rss_bytes.map(|measured| {
        let estimated = estimate.estimated_peak_memory_bytes.max(1) as f64;
        ((measured as f64 - estimated) / estimated * 1_000_000.0).round() / 1_000_000.0
    });
    let estimate_range_contains_measurement = measured_peak_rss_bytes.map(|measured| {
        let measured = u128::from(measured);
        measured >= estimate.estimated_peak_memory_low_bytes
            && measured <= estimate.estimated_peak_memory_high_bytes
    });
    let history_evidence_status = if repo.head_commit.is_none() {
        "not_applicable_unborn"
    } else if history_evidence_reliable {
        "supported_with_per_file_shrinkage"
    } else {
        "incomplete_suppressed"
    };
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
        observation_feed,
        diagnostics: json!({
            "analysis_elapsed_ms_before_report": started.elapsed().as_millis(),
            "estimate": estimate,
            "measured_peak_rss_bytes": measured_peak_rss_bytes,
            "estimator_error_ratio": estimator_error_ratio,
            "estimate_range_contains_measurement": estimate_range_contains_measurement,
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
            "intentionally_skipped_non_text_files": intentionally_skipped_non_text_files,
            "incomplete_inventory_files": incomplete_inventory_files,
            "analysis_status": if tracked_paths.len() < original_selected_path_count || !memory_budget_exceeded_checkpoints.is_empty() { "degraded_resource_budget" } else if structurally_skipped_large_files > 0 { "degraded_large_files" } else if incomplete_inventory_files > 0 { "degraded_incomplete_inventory" } else { "complete" },
            "resource_mode": if tracked_paths.len() < original_selected_path_count { "degraded_path_prefix" } else if !memory_budget_exceeded_checkpoints.is_empty() { "degraded_measured_rss" } else { "complete" },
            "original_selected_path_count": original_selected_path_count,
            "degraded_omitted_path_count": original_selected_path_count.saturating_sub(tracked_paths.len()),
            "history": history_diagnostics,
            "history_evidence_status": history_evidence_status,
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

include!("analyze/tests.rs");
