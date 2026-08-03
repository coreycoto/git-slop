use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tiktoken_rs::{
    CoreBPE, cl100k_base, o200k_base, o200k_harmony, p50k_base, p50k_edit, r50k_base,
};
use unicode_normalization::UnicodeNormalization;

use crate::config;
use crate::git;
use crate::health;
use crate::history;
use crate::inventory;
use crate::model::{Analysis, FileAnalysis, FindResult};
use crate::overlays;
use crate::report;
use crate::scoring;

static CAMEL_CASE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").expect("valid camel-case regex"));
static NUMBER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d+(?:\.\d+)?\b").expect("valid number regex"));
static WORD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-z][a-z0-9_]{1,}").expect("valid word regex"));

fn replace_quoted_strings(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if !matches!(character, '\'' | '"' | '`') {
            result.push(character);
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
    }
    result
}

fn structural_tokens(path: &str, text: &str) -> Vec<String> {
    let normalized: String = text.nfkc().collect();
    let normalized = CAMEL_CASE_RE.replace_all(&normalized, "$1 $2");
    let normalized = normalized.replace(['-', '/'], " ");
    let normalized = replace_quoted_strings(&normalized);
    let normalized = NUMBER_RE.replace_all(&normalized, " 0 ");
    let lower = normalized.to_ascii_lowercase();
    let mut tokens: Vec<String> = WORD_RE
        .find_iter(&lower)
        .map(|item| item.as_str().to_string())
        .collect();
    tokens.extend(
        path.replace(['-', '_', '.'], "/")
            .to_ascii_lowercase()
            .split('/')
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned),
    );
    tokens
}

fn content_fingerprint(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn top_terms(tokens: &[String]) -> Vec<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for token in tokens {
        *counts.entry(token).or_default() += 1;
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    ranked
        .into_iter()
        .take(12)
        .map(|(term, _)| term.to_string())
        .collect()
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
        .take(25)
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
    config::ensure_state_dirs(repo_root)?;
    let loaded_config = config::load(repo_root)?;
    let repo = git::repo_metadata(repo_root)?;
    let tracked_paths = git::list_tracked_files(repo_root)?;
    let (inventory_files, skipped) = inventory::build(repo_root, &tracked_paths, &loaded_config)?;
    let encoder = configured_context_encoder(&loaded_config)?;
    let mut token_counts = BTreeMap::new();
    let mut line_counts = BTreeMap::new();
    let mut token_data = HashMap::new();
    for file in &inventory_files {
        let count = encoder.encode_ordinary(&file.text).len();
        let structural = structural_tokens(&file.path, &file.text);
        let fingerprint = content_fingerprint(&file.text);
        token_counts.insert(file.path.clone(), count);
        line_counts.insert(file.path.clone(), file.lines);
        token_data.insert(
            file.path.clone(),
            (
                structural.len(),
                top_terms(&structural),
                structural,
                fingerprint,
            ),
        );
    }
    let analyzed_paths: Vec<String> = inventory_files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let now = Utc::now();
    let (history_by_path, commits, _repo_baselines) = history::analyze_history(
        repo_root,
        &analyzed_paths,
        &token_counts,
        &line_counts,
        &loaded_config,
        now,
    )?;
    let mut files = Vec::with_capacity(inventory_files.len());
    for file in inventory_files {
        let tokens = token_counts.get(&file.path).copied().unwrap_or_default();
        let (structural_token_count, top_structural_terms, structural_tokens, content_fingerprint) =
            token_data
                .remove(&file.path)
                .unwrap_or_else(|| (0, Vec::new(), Vec::new(), String::new()));
        let history = history_by_path.get(&file.path).cloned().unwrap_or_default();
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
            tokens,
            context_band: scoring::context_band_for_tokens(tokens, &loaded_config),
            context_pressure: scoring::context_pressure_for_tokens(tokens, &loaded_config),
            content_fingerprint,
            structural_tokens,
            structural_token_count,
            top_structural_terms,
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
    let folders = scoring::build_folder_analyses(&files, &loaded_config);
    let queue = action_queue(&files);
    let generated_at = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    let analyzed_revision_at = repo.head_commit_timestamp.clone();
    let analysis = Analysis {
        repo_root: PathBuf::from(repo_root),
        repo,
        config: loaded_config,
        generated_at,
        analyzed_revision_at,
        skipped,
        tracked_file_count: tracked_paths.len(),
        files,
        folders,
        commits,
        organization,
        action_queue: queue,
        report: Value::Null,
    };
    let rollup = health::build_health_rollup(&analysis);
    let result = report::write_report_bundle(&analysis, &rollup)?;
    if result.report.get("schema_version").and_then(Value::as_u64) != Some(4) {
        bail!("internal error: report writer did not produce schema 4");
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
            tokens: 100,
            context_band: "compact".to_string(),
            context_pressure: 0.0,
            content_fingerprint: String::new(),
            structural_tokens: Vec::new(),
            structural_token_count: 0,
            top_structural_terms: Vec::new(),
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
