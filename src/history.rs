use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{Value, json};

use crate::config::{pointer_bool, pointer_strings, pointer_u64};
use crate::model::{CommitRecord, HistoryMetrics, top_level_root};

const DEFAULT_HISTORY_WINDOW_DAYS: u64 = 180;
const RECENCY_HALF_WINDOW_DAYS: f64 = 30.0;
const LATE_CHURN_WINDOW_DAYS: i64 = 30;
const RECENT_MAINTAINER_WINDOW_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StatusChange {
    Path { status: String, path: String },
    Rename { old_path: String, new_path: String },
    Copy { old_path: String, new_path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusCommit {
    commit: String,
    timestamp: i64,
    author: String,
    parents: Vec<String>,
    subject: String,
    changes: Vec<StatusChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumstatEntry {
    added: usize,
    deleted: usize,
    paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumstatCommit {
    commit: String,
    timestamp: i64,
    author: String,
    parents: Vec<String>,
    subject: String,
    entries: Vec<NumstatEntry>,
}

#[derive(Debug, Clone)]
struct CommitFileChange {
    added: usize,
    deleted: usize,
    line_churn: usize,
    token_churn: usize,
}

#[derive(Debug, Default)]
struct FileAccumulator {
    metrics: HistoryMetrics,
    author_counts: BTreeMap<String, usize>,
    recent_authors: BTreeSet<String>,
    latest_non_bot_timestamp: Option<i64>,
    late_token_churn: usize,
}

#[derive(Debug)]
struct BaselineCommit {
    file_count: usize,
    total_token_delta: f64,
    top_level_root_count: usize,
    change_entropy: f64,
}

include!("history/parsing.rs");
include!("history/git_log.rs");
include!("history/lineage.rs");
include!("history/metrics.rs");
include!("history/analysis.rs");
include!("history/tests.rs");
