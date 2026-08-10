fn token_density(
    paths: &BTreeSet<String>,
    token_counts: &BTreeMap<String, usize>,
    line_counts: &BTreeMap<String, usize>,
) -> BTreeMap<String, f64> {
    paths
        .iter()
        .map(|path| {
            let lines = line_counts.get(path).copied().unwrap_or(0).max(1);
            let tokens = token_counts.get(path).copied().unwrap_or(0);
            (path.clone(), ((tokens as f64) / (lines as f64)).max(1.0))
        })
        .collect()
}

fn shannon_entropy(weights: impl IntoIterator<Item = usize>) -> f64 {
    let weights: Vec<usize> = weights.into_iter().collect();
    let total: usize = weights.iter().sum();
    if total == 0 {
        return 0.0;
    }
    weights
        .into_iter()
        .filter(|weight| *weight > 0)
        .map(|weight| {
            let probability = weight as f64 / total as f64;
            -probability * probability.log2()
        })
        .sum()
}

fn round_to(value: f64, digits: i32) -> f64 {
    let factor = 10_f64.powi(digits);
    (value * factor).round_ties_even() / factor
}

fn nearest_rank_percentile(values: &[f64], quantile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((sorted.len() as f64 * quantile).ceil() as usize).max(1);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn repo_baselines(commits: &[BaselineCommit]) -> Value {
    if commits.is_empty() {
        return json!({
            "p95_files_touched": 0.0,
            "p99_files_touched": 0.0,
            "p95_token_delta_mass": 0.0,
            "p95_top_level_root_spread": 0.0,
            "p95_change_entropy": 0.0,
        });
    }
    let files_touched: Vec<f64> = commits
        .iter()
        .map(|commit| commit.file_count as f64)
        .collect();
    let token_delta: Vec<f64> = commits
        .iter()
        .map(|commit| commit.total_token_delta)
        .collect();
    let root_spread: Vec<f64> = commits
        .iter()
        .map(|commit| commit.top_level_root_count as f64)
        .collect();
    let entropy: Vec<f64> = commits.iter().map(|commit| commit.change_entropy).collect();
    json!({
        "p95_files_touched": nearest_rank_percentile(&files_touched, 0.95),
        "p99_files_touched": nearest_rank_percentile(&files_touched, 0.99),
        "p95_token_delta_mass": nearest_rank_percentile(&token_delta, 0.95),
        "p95_top_level_root_spread": nearest_rank_percentile(&root_spread, 0.95),
        "p95_change_entropy": nearest_rank_percentile(&entropy, 0.95),
    })
}

fn is_bot(author: &str, markers: &[String]) -> bool {
    let author = author.to_lowercase();
    markers.iter().any(|marker| author.contains(marker))
}

fn empty_result(
    analyzed_paths: &[String],
    status: &str,
) -> (BTreeMap<String, HistoryMetrics>, Vec<CommitRecord>, Value) {
    let metrics = analyzed_paths
        .iter()
        .map(|path| (normalized_path(path), HistoryMetrics::default()))
        .collect();
    let mut diagnostics = repo_baselines(&[]);
    diagnostics["history_status"] = json!(status);
    diagnostics["history_cap_reached"] = json!(false);
    (metrics, Vec::new(), diagnostics)
}
