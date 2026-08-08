use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::config::pointer_u64;
use crate::model::{CommitRecord, FileAnalysis, top_level_root};
use serde_json::Value;

use super::common::round6;

const PAGERANK_ITERATIONS: usize = 20;
const PAGERANK_DAMPING: f64 = 0.85;

#[derive(Default)]
pub(super) struct CoordinationFacts {
    pub(super) commit_count: usize,
    pub(super) touched_file_total: usize,
    pub(super) touched_folder_total: usize,
    pub(super) line_hunks_total: usize,
    pub(super) diffusion_total: f64,
    pub(super) neighbors: BTreeMap<String, usize>,
}

fn estimated_hunks(line_delta: usize) -> usize {
    line_delta.max(1).div_ceil(20)
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

pub(super) fn cochange_pagerank(
    coordination: &BTreeMap<String, CoordinationFacts>,
) -> BTreeMap<String, f64> {
    let nodes: Vec<&String> = coordination
        .iter()
        .filter_map(|(path, facts)| (!facts.neighbors.is_empty()).then_some(path))
        .collect();
    if nodes.is_empty() {
        return BTreeMap::new();
    }

    let node_count = nodes.len() as f64;
    let mut ranks: BTreeMap<String, f64> = nodes
        .iter()
        .map(|path| ((*path).clone(), 1.0 / node_count))
        .collect();
    for _ in 0..PAGERANK_ITERATIONS {
        let mut next_ranks: BTreeMap<String, f64> = nodes
            .iter()
            .map(|path| ((*path).clone(), (1.0 - PAGERANK_DAMPING) / node_count))
            .collect();
        for path in &nodes {
            let Some(facts) = coordination.get(*path) else {
                continue;
            };
            let total_weight: usize = facts.neighbors.values().sum();
            if total_weight == 0 {
                continue;
            }
            let rank = ranks.get(*path).copied().unwrap_or_default();
            for (neighbor, support) in &facts.neighbors {
                let Some(next_rank) = next_ranks.get_mut(neighbor) else {
                    continue;
                };
                let weight = *support as f64 / total_weight as f64;
                *next_rank += PAGERANK_DAMPING * rank * weight;
            }
        }
        ranks = next_ranks;
    }
    ranks
}

pub(super) fn coordination_facts(
    files: &[FileAnalysis],
    commits: &[CommitRecord],
    config: &Value,
) -> BTreeMap<String, CoordinationFacts> {
    let max_commit_files = pointer_u64(config, "/organization/max_commit_files", 200) as usize;
    let max_neighbors = pointer_u64(config, "/organization/max_pairs_per_file", 20) as usize;
    let paths: HashSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
    let mut result: BTreeMap<String, CoordinationFacts> = files
        .iter()
        .map(|file| (file.path.clone(), CoordinationFacts::default()))
        .collect();
    for commit in commits {
        let mut touched: Vec<&str> = commit
            .paths
            .iter()
            .map(String::as_str)
            .filter(|path| paths.contains(path))
            .collect();
        touched.sort_unstable();
        touched.dedup();
        if touched.len() > max_commit_files {
            continue;
        }
        let roots: BTreeSet<String> = touched.iter().map(|path| top_level_root(path)).collect();
        let total_hunks: usize = touched
            .iter()
            .map(|path| {
                estimated_hunks(
                    commit
                        .line_churn_by_path
                        .get(*path)
                        .copied()
                        .unwrap_or_default(),
                )
            })
            .sum();
        let entropy = round6(shannon_entropy(touched.iter().map(|path| {
            commit
                .line_churn_by_path
                .get(*path)
                .copied()
                .unwrap_or_default()
        })));
        let file_count = touched.len().max(1);
        let root_count = roots.len().max(1);
        let diffusion = 0.35 * ((file_count as f64).ln_1p() / 25_f64.ln()).min(1.0)
            + 0.25 * ((total_hunks as f64).ln_1p() / 50_f64.ln()).min(1.0)
            + 0.20 * ((root_count as f64).ln_1p() / 10_f64.ln()).min(1.0)
            + 0.20 * (entropy / 3.0).min(1.0);
        for source in &touched {
            let Some(facts) = result.get_mut(*source) else {
                continue;
            };
            facts.commit_count += 1;
            facts.touched_file_total += file_count;
            facts.touched_folder_total += root_count;
            facts.line_hunks_total += estimated_hunks(
                commit
                    .line_churn_by_path
                    .get(*source)
                    .copied()
                    .unwrap_or_default(),
            );
            facts.diffusion_total += diffusion;
            for target in &touched {
                if source != target {
                    *facts.neighbors.entry((*target).to_string()).or_default() += 1;
                }
            }
        }
    }
    for facts in result.values_mut() {
        if facts.neighbors.len() > max_neighbors {
            let mut ranked: Vec<(String, usize)> =
                std::mem::take(&mut facts.neighbors).into_iter().collect();
            ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            ranked.truncate(max_neighbors);
            facts.neighbors = ranked.into_iter().collect();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{CoordinationFacts, cochange_pagerank};
    use crate::overlays::common::round6;

    #[test]
    fn weighted_cochange_pagerank_matches_the_stable_two_node_result() {
        let coordination = BTreeMap::from([
            (
                "src/a.rs".to_string(),
                CoordinationFacts {
                    neighbors: BTreeMap::from([("src/b.rs".to_string(), 3)]),
                    ..CoordinationFacts::default()
                },
            ),
            (
                "src/b.rs".to_string(),
                CoordinationFacts {
                    neighbors: BTreeMap::from([("src/a.rs".to_string(), 3)]),
                    ..CoordinationFacts::default()
                },
            ),
        ]);
        let pagerank = cochange_pagerank(&coordination);

        assert_eq!(round6(pagerank["src/a.rs"]), 0.5);
        assert_eq!(round6(pagerank["src/b.rs"]), 0.5);
    }
}
