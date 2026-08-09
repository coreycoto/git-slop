use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisEstimate {
    pub tracked_path_count: usize,
    pub inventory_bytes: u128,
    pub estimated_peak_memory_bytes: u128,
    pub memory_budget_bytes: u128,
    pub estimated_cache_bytes: u128,
    pub estimated_report_bytes: u128,
    pub estimated_inode_count: u128,
    pub estimated_seconds: u128,
}

pub fn build(repo_root: &Path, paths: &[String], config_value: &Value) -> AnalysisEstimate {
    let inventory_bytes = paths
        .iter()
        .filter_map(|path| fs::metadata(repo_root.join(path)).ok())
        .map(|metadata| metadata.len() as u128)
        .sum::<u128>();
    let path_count = paths.len() as u128;
    let pair_limit =
        config::pointer_u64(config_value, "/organization/max_pairs_per_file", 20) as u128;
    let graph_bytes = path_count.saturating_mul(pair_limit).saturating_mul(256);
    let history_bytes = path_count.saturating_mul(1_024);
    let tokenizer_bytes = inventory_bytes.saturating_mul(2);
    let report_bytes = inventory_bytes
        .saturating_div(2)
        .saturating_add(path_count.saturating_mul(4_096));
    let estimated_peak_memory_bytes = inventory_bytes
        .saturating_add(tokenizer_bytes)
        .saturating_add(graph_bytes)
        .saturating_add(history_bytes)
        .saturating_add(report_bytes);
    AnalysisEstimate {
        tracked_path_count: paths.len(),
        inventory_bytes,
        estimated_peak_memory_bytes,
        memory_budget_bytes: config::pointer_u64(config_value, "/resources/memory_budget_mb", 1024)
            as u128
            * 1024
            * 1024,
        estimated_cache_bytes: inventory_bytes
            .saturating_div(3)
            .saturating_add(path_count * 512),
        estimated_report_bytes: report_bytes,
        estimated_inode_count: path_count.saturating_add(16),
        estimated_seconds: inventory_bytes
            .div_ceil(8 * 1024 * 1024)
            .saturating_add(path_count.div_ceil(2_000))
            .max(1),
    }
}
