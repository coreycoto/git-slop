use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Value, json};

use crate::config::{pointer_f64, pointer_u64};
use crate::model::{FileAnalysis, top_level_root};

use super::common::{
    ORGANIZATION_ANALYSIS_STATUS, ORGANIZATION_ANALYSIS_VERSION, jaccard, round6, stable_id,
};
use super::coordination::CoordinationFacts;

fn shingles(tokens: &[String], size: usize, step: usize) -> Vec<String> {
    if tokens.len() < size {
        return tokens.to_vec();
    }
    tokens
        .windows(size)
        .step_by(step.max(1))
        .map(|window| window.join("\u{1f}"))
        .collect()
}

fn organization_candidates<'a>(files: &'a [FileAnalysis], config: &Value) -> Vec<&'a FileAnalysis> {
    let limit = pointer_u64(config, "/organization/candidate_file_limit", 500) as usize;
    let min_tokens = pointer_u64(config, "/organization/min_file_tokens", 300) as usize;
    let max_tokens = pointer_u64(config, "/organization/max_file_tokens", 50_000) as usize;
    let mut candidates: Vec<&FileAnalysis> = files
        .iter()
        .filter(|file| {
            file.structural_token_count >= min_tokens && file.structural_token_count <= max_tokens
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .structural_token_count
            .cmp(&left.structural_token_count)
            .then_with(|| left.path.cmp(&right.path))
    });
    candidates.truncate(limit);
    candidates
}

pub(super) fn build_relationships(
    files: &[FileAnalysis],
    coordination: &BTreeMap<String, CoordinationFacts>,
    config: &Value,
) -> (Value, Vec<Value>, BTreeMap<String, Vec<String>>) {
    let min_similarity = pointer_f64(config, "/organization/min_similarity", 0.72);
    let max_pairs = pointer_u64(config, "/organization/max_pairs_per_file", 20) as usize;
    let shingle_size = pointer_u64(config, "/organization/shingle_size", 8) as usize;
    let window_step = pointer_u64(config, "/organization/window_step", 32) as usize;
    let candidates = organization_candidates(files, config);

    let mut duplicate = Vec::new();
    let mut near_duplicate = Vec::new();
    let mut relationship_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pair_counts: HashMap<String, usize> = HashMap::new();
    for (index, left) in candidates.iter().enumerate() {
        for right in candidates.iter().skip(index + 1) {
            if pair_counts.get(&left.path).copied().unwrap_or_default() >= max_pairs
                || pair_counts.get(&right.path).copied().unwrap_or_default() >= max_pairs
            {
                continue;
            }
            let left_shingles = shingles(&left.structural_tokens, shingle_size, window_step);
            let right_shingles = shingles(&right.structural_tokens, shingle_size, window_step);
            let similarity = jaccard(&left_shingles, &right_shingles);
            let exact = !left.content_fingerprint.is_empty()
                && left.content_fingerprint == right.content_fingerprint;
            if !exact && similarity < min_similarity {
                continue;
            }
            let relationship_similarity = if exact { 1.0 } else { similarity };
            let kind = if exact {
                "duplicate_neighborhood"
            } else {
                "near_duplicate_neighborhood"
            };
            let (source, target) = if left.path <= right.path {
                (left.path.as_str(), right.path.as_str())
            } else {
                (right.path.as_str(), left.path.as_str())
            };
            let id = stable_id(kind, &[source, target]);
            let item = json!({
                "id": id,
                "kind": kind,
                "source_path": source,
                "target_path": target,
                "evidence_score": round6(relationship_similarity),
                "similarity": round6(relationship_similarity),
                "crosses_top_level_boundary": top_level_root(source) != top_level_root(target)
            });
            relationship_ids
                .entry(source.to_string())
                .or_default()
                .push(id.clone());
            relationship_ids
                .entry(target.to_string())
                .or_default()
                .push(id.clone());
            *pair_counts.entry(source.to_string()).or_default() += 1;
            *pair_counts.entry(target.to_string()).or_default() += 1;
            if exact {
                duplicate.push(item);
            } else {
                near_duplicate.push(item);
            }
        }
    }

    let min_support = pointer_u64(config, "/organization/min_cochange_support", 3) as usize;
    let min_lift = pointer_f64(config, "/organization/min_coupling_lift", 2.0);
    let max_temporal_edges =
        pointer_u64(config, "/organization/max_temporal_edges", 10_000) as usize;
    let mut temporal = Vec::new();
    let mut seen_pairs = BTreeSet::new();
    for (source, facts) in coordination {
        for (target, support) in &facts.neighbors {
            let pair = if source <= target {
                (source.as_str(), target.as_str())
            } else {
                (target.as_str(), source.as_str())
            };
            if !seen_pairs.insert((pair.0.to_string(), pair.1.to_string()))
                || *support < min_support
            {
                continue;
            }
            let target_commits = coordination
                .get(target)
                .map(|item| item.commit_count)
                .unwrap_or(1);
            let denom = facts.commit_count.min(target_commits).max(1);
            let coupling = *support as f64 / denom as f64;
            let lift = 1.0 + coupling * 4.0;
            if lift < min_lift {
                continue;
            }
            let id = stable_id("temporal_coupling_edge", &[pair.0, pair.1]);
            let item = json!({
                "id": id,
                "kind": "temporal_coupling_edge",
                "source_path": pair.0,
                "target_path": pair.1,
                "support_count": support,
                "lift_score": round6(lift),
                "evidence_score": round6(coupling),
                "crosses_top_level_boundary": top_level_root(pair.0) != top_level_root(pair.1)
            });
            relationship_ids
                .entry(pair.0.to_string())
                .or_default()
                .push(id.clone());
            relationship_ids
                .entry(pair.1.to_string())
                .or_default()
                .push(id);
            temporal.push(item);
        }
    }
    temporal.sort_by(|left, right| {
        right["evidence_score"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&left["evidence_score"].as_f64().unwrap_or_default())
            .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
    });
    temporal.truncate(max_temporal_edges);

    let mut lexical = Vec::new();
    for (index, left) in candidates.iter().enumerate() {
        for right in candidates.iter().skip(index + 1) {
            if top_level_root(&left.path) == top_level_root(&right.path) {
                continue;
            }
            let similarity = jaccard(&left.top_structural_terms, &right.top_structural_terms);
            if similarity < 0.35 {
                continue;
            }
            let (source, target) = if left.path <= right.path {
                (left.path.as_str(), right.path.as_str())
            } else {
                (right.path.as_str(), left.path.as_str())
            };
            lexical.push(json!({
                "id": stable_id("lexical_affinity_edge", &[source, target]),
                "kind": "lexical_affinity_edge",
                "source_path": source,
                "target_path": target,
                "evidence_score": round6(similarity),
                "crosses_top_level_boundary": true
            }));
        }
    }
    lexical.sort_by(|left, right| {
        right["evidence_score"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&left["evidence_score"].as_f64().unwrap_or_default())
            .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
    });
    lexical.truncate(100);
    for ids in relationship_ids.values_mut() {
        ids.sort();
        ids.dedup();
    }

    duplicate.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    near_duplicate.sort_by(|left, right| {
        right["evidence_score"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&left["evidence_score"].as_f64().unwrap_or_default())
            .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
    });
    let all_duplicate: Vec<Value> = duplicate
        .iter()
        .chain(near_duplicate.iter())
        .cloned()
        .collect();
    (
        json!({
            "analysis_status": ORGANIZATION_ANALYSIS_STATUS,
            "analysis_version": ORGANIZATION_ANALYSIS_VERSION,
            "duplicate_neighborhoods": duplicate,
            "near_duplicate_neighborhoods": near_duplicate,
            "temporal_coupling_edges": temporal,
            "lexical_affinity_edges": lexical,
            "boundary_leakage_edges": []
        }),
        all_duplicate,
        relationship_ids,
    )
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    use super::{build_relationships, shingles};
    use crate::model::FileAnalysis;
    use crate::overlays::coordination::{CoordinationFacts, coordination_facts};

    fn test_file(index: usize) -> FileAnalysis {
        FileAnalysis {
            path: format!("root{index:03}/file.rs"),
            bytes: 1_000,
            lines: 100,
            blank_lines: 0,
            code_lines: 100,
            comment_lines: 0,
            language: "Rust".to_string(),
            profile: "agent_context".to_string(),
            classification: "source".to_string(),
            has_inline_tests: false,
            tokens: 500,
            context_band: "compact".to_string(),
            context_pressure: 0.1,
            content_fingerprint: format!("fingerprint-{index}"),
            structural_tokens: vec![format!("unique-{index}")],
            structural_token_count: 300,
            top_structural_terms: vec!["shared".to_string()],
            age_days: 0,
            revisions_window: 0,
            recency_weighted_commits: 0.0,
            added_window: 0,
            deleted_window: 0,
            churn_lines_window: 0,
            line_churn_window: 0,
            token_churn_window: 0,
            relative_churn_window: 0.0,
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
            slop_band: "low".to_string(),
            reason_codes: Vec::new(),
            costs: json!({}),
            overlays: json!({}),
        }
    }

    #[test]
    fn lexical_candidate_pairs_are_bounded_by_the_configured_file_limit() {
        let files: Vec<FileAnalysis> = (0..100).map(test_file).collect();
        let config = json!({
            "organization": {
                "candidate_file_limit": 5,
                "min_file_tokens": 0,
                "max_file_tokens": 50_000,
                "min_similarity": 2.0,
                "max_pairs_per_file": 1_000,
                "min_cochange_support": 3
            }
        });
        let coordination = coordination_facts(&files, &[], &config);
        let (relationships, _, _) = build_relationships(&files, &coordination, &config);

        assert_eq!(relationships["analysis_version"], 2);
        assert_eq!(
            relationships["lexical_affinity_edges"]
                .as_array()
                .map(Vec::len),
            Some(10)
        );
        assert!(
            relationships["lexical_affinity_edges"]
                .as_array()
                .expect("lexical edges")
                .iter()
                .all(|edge| edge["kind"] == "lexical_affinity_edge")
        );
        for key in [
            "duplicate_neighborhoods",
            "near_duplicate_neighborhoods",
            "temporal_coupling_edges",
            "lexical_affinity_edges",
            "boundary_leakage_edges",
        ] {
            assert!(
                relationships[key].is_array(),
                "missing canonical array {key}"
            );
        }
    }

    #[test]
    fn relationship_metadata_uses_canonical_v2_contract() {
        let files: Vec<FileAnalysis> = (0..2).map(test_file).collect();
        let config: Value = json!({
            "organization": {
                "candidate_file_limit": 2,
                "min_file_tokens": 0,
                "max_file_tokens": 50_000,
                "min_similarity": 0.0,
                "max_pairs_per_file": 20,
                "min_cochange_support": 3
            }
        });
        let coordination = coordination_facts(&files, &[], &config);
        let (relationships, _, _) = build_relationships(&files, &coordination, &config);

        assert_eq!(relationships["analysis_status"], "experimental");
        assert_eq!(relationships["analysis_version"], 2);
    }

    #[test]
    fn shingle_size_and_window_step_are_behaviorally_live() {
        let tokens = (0..12)
            .map(|index| format!("token{index}"))
            .collect::<Vec<_>>();
        assert_ne!(shingles(&tokens, 2, 1), shingles(&tokens, 3, 1));
        assert_ne!(shingles(&tokens, 2, 1), shingles(&tokens, 2, 3));
    }

    #[test]
    fn minimum_coupling_lift_filters_temporal_edges() {
        let files: Vec<FileAnalysis> = (0..2).map(test_file).collect();
        let coordination = BTreeMap::from([
            (
                files[0].path.clone(),
                CoordinationFacts {
                    commit_count: 10,
                    neighbors: BTreeMap::from([(files[1].path.clone(), 3)]),
                    ..CoordinationFacts::default()
                },
            ),
            (
                files[1].path.clone(),
                CoordinationFacts {
                    commit_count: 10,
                    neighbors: BTreeMap::from([(files[0].path.clone(), 3)]),
                    ..CoordinationFacts::default()
                },
            ),
        ]);
        let base = json!({"organization": {"min_file_tokens": 0, "max_file_tokens": 50000, "candidate_file_limit": 2, "min_similarity": 2.0, "max_pairs_per_file": 20, "min_cochange_support": 3, "min_coupling_lift": 2.0}});
        let strict = json!({"organization": {"min_file_tokens": 0, "max_file_tokens": 50000, "candidate_file_limit": 2, "min_similarity": 2.0, "max_pairs_per_file": 20, "min_cochange_support": 3, "min_coupling_lift": 3.0}});
        assert_eq!(
            build_relationships(&files, &coordination, &base).0["temporal_coupling_edges"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            build_relationships(&files, &coordination, &strict).0["temporal_coupling_edges"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn content_identity_is_exact_even_when_public_tokens_include_distinct_paths() {
        let mut files: Vec<FileAnalysis> = (0..2).map(test_file).collect();
        files[0].content_fingerprint = "same-content".to_string();
        files[1].content_fingerprint = "same-content".to_string();
        files[0].structural_tokens = vec!["shared".to_string(), "root000".to_string()];
        files[1].structural_tokens = vec!["shared".to_string(), "root001".to_string()];
        assert_ne!(files[0].structural_tokens, files[1].structural_tokens);

        let serialized = serde_json::to_value(&files[0]).expect("serialize file analysis");
        assert!(serialized.get("content_fingerprint").is_none());
        assert!(serialized.get("structural_tokens").is_none());

        let config = json!({
            "organization": {
                "candidate_file_limit": 2,
                "min_file_tokens": 0,
                "max_file_tokens": 50_000,
                "min_similarity": 1.0,
                "max_pairs_per_file": 20,
                "min_cochange_support": 3
            }
        });
        let coordination = coordination_facts(&files, &[], &config);
        let (relationships, _, _) = build_relationships(&files, &coordination, &config);

        assert_eq!(
            relationships["duplicate_neighborhoods"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            relationships["duplicate_neighborhoods"][0]["similarity"],
            1.0
        );
        assert_eq!(
            relationships["near_duplicate_neighborhoods"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }
}
