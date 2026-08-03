use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMetadata {
    pub repo_name: String,
    pub repo_root: String,
    pub branch: Option<String>,
    pub head_commit: Option<String>,
    pub head_commit_timestamp: Option<String>,
    pub git_remote_url: Option<String>,
    pub is_shallow: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkippedCounts {
    pub ignored: usize,
    pub missing: usize,
    pub binary: usize,
    pub undecodable: usize,
}

#[derive(Debug, Clone)]
pub struct InventoryFile {
    pub path: String,
    pub absolute_path: PathBuf,
    pub bytes: usize,
    pub lines: usize,
    pub blank_lines: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub language: String,
    pub profile: String,
    pub classification: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryMetrics {
    pub first_seen_timestamp: Option<i64>,
    pub age_days: u64,
    pub revisions_window: usize,
    pub recency_weighted_commits: f64,
    pub added_window: usize,
    pub deleted_window: usize,
    pub line_churn_window: usize,
    pub token_churn_window: usize,
    pub relative_churn_window: f64,
    pub late_churn_spike: f64,
    pub author_count_window: usize,
    pub author_entropy: f64,
    pub top_author_share: f64,
    pub days_since_non_bot_edit: Option<u64>,
    pub recent_maintainer_diversity: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRecord {
    pub commit: String,
    pub timestamp: i64,
    pub author: String,
    pub paths: Vec<String>,
    pub line_churn_by_path: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    pub path: String,
    pub bytes: usize,
    pub lines: usize,
    pub blank_lines: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub language: String,
    pub profile: String,
    pub classification: String,
    pub tokens: usize,
    pub context_band: String,
    pub context_pressure: f64,
    #[serde(skip)]
    pub content_fingerprint: String,
    pub structural_tokens: Vec<String>,
    pub structural_token_count: usize,
    pub top_structural_terms: Vec<String>,
    pub age_days: u64,
    pub revisions_window: usize,
    pub recency_weighted_commits: f64,
    pub added_window: usize,
    pub deleted_window: usize,
    pub churn_lines_window: usize,
    pub line_churn_window: usize,
    pub token_churn_window: usize,
    pub relative_churn_window: f64,
    pub late_churn_spike: f64,
    pub author_count_window: usize,
    pub author_entropy: f64,
    pub top_author_share: f64,
    pub days_since_non_bot_edit: Option<u64>,
    pub recent_maintainer_diversity: usize,
    pub age_pressure: f64,
    pub revision_norm: f64,
    pub relative_churn_norm: f64,
    pub churn_pressure: f64,
    pub slop_score: f64,
    pub slop_band: String,
    pub reason_codes: Vec<String>,
    pub costs: Value,
    pub overlays: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderAnalysis {
    pub path: String,
    pub descendant_file_count: usize,
    pub direct_file_count: usize,
    pub bytes: usize,
    pub lines: usize,
    pub tokens: usize,
    pub direct_tokens: usize,
    pub context_band: String,
    pub health_band: String,
    pub context_pressure: f64,
    pub slop_score: f64,
    pub slop_band: String,
    pub reason_codes: Vec<String>,
    pub top_file_path: String,
    pub classification: String,
    pub costs: Value,
    pub overlays: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrganizationAnalysis {
    pub organization_metrics: Value,
    pub relationships: Value,
    pub clusters: Value,
    pub file_overlays: BTreeMap<String, Value>,
    pub folder_overlays: BTreeMap<String, Value>,
    pub top_structural_files: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub repo_root: PathBuf,
    pub repo: RepoMetadata,
    pub config: Value,
    pub generated_at: String,
    pub analyzed_revision_at: Option<String>,
    pub skipped: SkippedCounts,
    pub tracked_file_count: usize,
    pub files: Vec<FileAnalysis>,
    pub folders: Vec<FolderAnalysis>,
    pub commits: Vec<CommitRecord>,
    pub organization: OrganizationAnalysis,
    pub action_queue: Vec<Value>,
    pub report: Value,
}

#[derive(Debug, Clone)]
pub struct FindResult {
    pub report: Value,
    pub report_json: PathBuf,
    pub report_yaml: PathBuf,
    pub summary_md: PathBuf,
    pub health_md: PathBuf,
    pub terminal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub path: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub next_command: String,
    pub slop_band: String,
    pub context_band: String,
    pub slop_score: f64,
    pub tokens: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthRollup {
    pub file_band_counts: BTreeMap<String, usize>,
    pub folder_band_counts: BTreeMap<String, usize>,
    pub profile_rollups: Vec<Value>,
    pub language_rollups: BTreeMap<String, Vec<Value>>,
    pub file_distribution: Value,
    pub folder_distribution: Value,
    pub refactor_candidates: Vec<Value>,
    pub watchlist: Vec<Value>,
    pub findings: Vec<Finding>,
}

pub fn parent_folders(path: &str) -> Vec<String> {
    let normalized = path.trim_matches('/');
    let mut result = vec![".".to_string()];
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() <= 1 {
        return result;
    }
    for index in 1..parts.len() {
        result.push(parts[..index].join("/"));
    }
    result
}

pub fn top_level_root(path: &str) -> String {
    path.trim_matches('/')
        .split('/')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(".")
        .to_string()
}

pub fn dedupe_ordered(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}
