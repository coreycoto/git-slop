use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;
use globset::Glob;
use serde_json::{Value, json};

use crate::config::{pointer_strings, pointer_u64};
use crate::model::{CommitRecord, FileAnalysis, OrganizationAnalysis, top_level_root};

mod clusters;
mod common;
mod coordination;
mod folders;
mod relationships;

use clusters::build_clusters;
use common::{
    ORGANIZATION_ANALYSIS_STATUS, ORGANIZATION_ANALYSIS_VERSION, immediate_parent, is_test_path,
    percentile, round6,
};
use coordination::{cochange_pagerank, coordination_facts};
use folders::folder_overlay_map;
use relationships::build_relationships;

fn language_common_term(term: &str) -> bool {
    matches!(
        term,
        "let"
            | "if"
            | "else"
            | "for"
            | "while"
            | "match"
            | "return"
            | "self"
            | "this"
            | "true"
            | "false"
            | "none"
            | "null"
            | "some"
            | "result"
            | "string"
            | "str"
            | "path"
            | "format"
            | "from"
            | "into"
            | "impl"
            | "pub"
            | "use"
            | "mod"
            | "fn"
            | "const"
            | "static"
            | "class"
            | "def"
            | "func"
            | "var"
            | "import"
            | "export"
    )
}

fn verification_override<'a>(path: &str, config: &'a Value) -> Option<&'a str> {
    config
        .pointer("/inventory/path_overrides")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|mapping| {
            mapping
                .get("glob")
                .and_then(Value::as_str)
                .and_then(|pattern| Glob::new(pattern).ok())
                .is_some_and(|glob| glob.compile_matcher().is_match(path))
        })
        .filter_map(|mapping| {
            mapping
                .get("verification_applicability")
                .and_then(Value::as_str)
        })
        .next_back()
}

pub fn analyze(
    files: &mut [FileAnalysis],
    commits: &[CommitRecord],
    config: &Value,
) -> Result<OrganizationAnalysis> {
    let coordination = coordination_facts(files, commits, config);
    let pagerank = cochange_pagerank(&coordination);
    let file_count = files.len().max(1);
    let total_tokens = files.iter().map(|file| file.tokens).sum::<usize>().max(1);
    let mut folder_tokens: BTreeMap<String, usize> = BTreeMap::new();
    let mut folder_children: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for file in files.iter() {
        let folder = immediate_parent(&file.path);
        *folder_tokens.entry(folder.clone()).or_default() += file.tokens;
        folder_children.entry(folder).or_default().push(file.tokens);
    }
    for child_tokens in folder_children.values_mut() {
        child_tokens.sort_unstable_by(|left, right| right.cmp(left));
    }
    let diffusion_p95 = percentile(
        coordination
            .values()
            .map(|facts| {
                if facts.commit_count == 0 {
                    0.0
                } else {
                    facts.touched_file_total as f64 / facts.commit_count as f64
                }
            })
            .collect(),
        0.95,
    )
    .max(1.0);
    let (relationships, duplicate_relationships, relationship_ids) =
        build_relationships(files, &coordination, config);
    let (clusters, cluster_ids) = build_clusters(&duplicate_relationships);

    let filename_counts: HashMap<String, usize> =
        files.iter().fold(HashMap::new(), |mut map, file| {
            let name = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_string();
            *map.entry(name).or_default() += 1;
            map
        });
    let sibling_counts: HashMap<String, usize> =
        files.iter().fold(HashMap::new(), |mut map, file| {
            *map.entry(immediate_parent(&file.path)).or_default() += 1;
            map
        });
    let test_markers = pointer_strings(config, "/verification/test_path_markers");
    let configured_test_path = |path: &str| {
        let lower = path.to_ascii_lowercase();
        is_test_path(path)
            || test_markers
                .iter()
                .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
    };
    let test_paths: Vec<String> = files
        .iter()
        .filter(|file| configured_test_path(&file.path))
        .map(|file| file.path.clone())
        .collect();
    let source_test_mappings = config
        .pointer("/verification/source_test_mappings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|mapping| {
            let source = Glob::new(mapping.get("source_glob")?.as_str()?)
                .ok()?
                .compile_matcher();
            let test = Glob::new(mapping.get("test_glob")?.as_str()?)
                .ok()?
                .compile_matcher();
            Some((source, test))
        })
        .collect::<Vec<_>>();
    let mut term_roots: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut term_documents: HashMap<String, usize> = HashMap::new();
    for file in files.iter() {
        let root = top_level_root(&file.path);
        let unique_terms = file.top_structural_terms.iter().collect::<BTreeSet<_>>();
        for term in unique_terms {
            *term_documents.entry(term.clone()).or_default() += 1;
            term_roots
                .entry(term.clone())
                .or_default()
                .insert(root.clone());
        }
    }

    let mut top_structural_files = Vec::new();
    let mut file_overlays = BTreeMap::new();
    for file in files.iter_mut() {
        let facts = coordination.get(&file.path);
        let commit_count = facts.map(|item| item.commit_count).unwrap_or_default();
        let average_files = facts
            .filter(|item| item.commit_count > 0)
            .map(|item| item.touched_file_total as f64 / item.commit_count as f64)
            .unwrap_or(1.0);
        let average_folders = facts
            .filter(|item| item.commit_count > 0)
            .map(|item| item.touched_folder_total as f64 / item.commit_count as f64)
            .unwrap_or(1.0);
        let average_hunks = facts
            .filter(|item| item.commit_count > 0)
            .map(|item| item.line_hunks_total as f64 / item.commit_count as f64)
            .unwrap_or(1.0);
        let change_diffusion = facts
            .filter(|item| item.commit_count > 0)
            .map(|item| item.diffusion_total / item.commit_count as f64)
            .unwrap_or_default();
        let neighbors = facts.map(|item| &item.neighbors);
        let degree = neighbors.map(BTreeMap::len).unwrap_or_default();
        let centrality = degree as f64 / file_count.saturating_sub(1).max(1) as f64;
        let cochange_pagerank = pagerank.get(&file.path).copied().unwrap_or_default();
        let cross_edges = neighbors
            .map(|items| {
                items
                    .keys()
                    .filter(|target| top_level_root(target) != top_level_root(&file.path))
                    .count()
            })
            .unwrap_or_default();
        let cross_ratio = if degree == 0 {
            0.0
        } else {
            cross_edges as f64 / degree as f64
        };
        let related_duplicates: Vec<&Value> = duplicate_relationships
            .iter()
            .filter(|item| {
                item["source_path"].as_str() == Some(&file.path)
                    || item["target_path"].as_str() == Some(&file.path)
            })
            .collect();
        let duplication_pressure = related_duplicates
            .iter()
            .filter_map(|item| item["evidence_score"].as_f64())
            .fold(0.0, f64::max);
        let diffusion_pressure = change_diffusion;
        let coupling_pressure = centrality.min(1.0);
        let boundary_pressure = cross_ratio;
        let coordination_pressure =
            (0.5 * change_diffusion + 0.3 * centrality + 0.2 * cross_ratio).min(1.0);

        let stem = file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_start_matches("test_")
            .trim_end_matches("_test");
        let mut nearby_tests: Vec<String> = test_paths
            .iter()
            .filter(|path| {
                let lower = path.to_ascii_lowercase();
                !stem.is_empty() && lower.contains(&stem.to_ascii_lowercase())
            })
            .cloned()
            .collect();
        for (source, test) in &source_test_mappings {
            if source.is_match(&file.path) {
                nearby_tests.extend(
                    test_paths
                        .iter()
                        .filter(|path| test.is_match(path))
                        .cloned(),
                );
            }
        }
        nearby_tests.sort();
        nearby_tests.dedup();
        nearby_tests.truncate(5);
        let test_adjacency = if configured_test_path(&file.path)
            || file.has_inline_tests
            || !nearby_tests.is_empty()
        {
            1.0
        } else {
            0.0
        };
        let test_neighbors = neighbors
            .map(|items| {
                items
                    .keys()
                    .filter(|path| configured_test_path(path))
                    .count()
            })
            .unwrap_or_default();
        let test_cochange_ratio = if degree == 0 {
            0.0
        } else {
            test_neighbors as f64 / degree as f64
        };
        let hotspot_without_nearby_tests = !configured_test_path(&file.path)
            && !file.has_inline_tests
            && file.slop_score >= 50.0
            && nearby_tests.is_empty();
        let verification_applicability =
            if verification_override(&file.path, config) == Some("not_applicable") {
                "not_applicable_override"
            } else if verification_override(&file.path, config) == Some("applicable") {
                "applicable"
            } else if configured_test_path(&file.path) {
                "not_applicable_test"
            } else if matches!(file.classification.as_str(), "source" | "tool") {
                "applicable"
            } else {
                "not_applicable_non_source"
            };
        let verification_gap = if verification_applicability != "applicable" {
            None
        } else if file.has_inline_tests {
            Some(0.0)
        } else {
            Some(
                ((1.0 - test_adjacency) * 0.6
                    + (1.0 - test_cochange_ratio.min(1.0)) * 0.2
                    + if hotspot_without_nearby_tests {
                        0.2
                    } else {
                        0.0
                    })
                .min(1.0),
            )
        };

        let path_depth = file.path.matches('/').count() + 1;
        let parent = immediate_parent(&file.path);
        let sibling_count = sibling_counts.get(&parent).copied().unwrap_or(1);
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        let duplicate_name_count = filename_counts.get(name).copied().unwrap_or(1);
        let search_ambiguity = ((duplicate_name_count.saturating_sub(1)) as f64 / 5.0).min(1.0);
        let navigation_pressure = ((path_depth.saturating_sub(1)) as f64 / 8.0 * 0.35
            + sibling_count.saturating_sub(1) as f64 / 30.0 * 0.35
            + search_ambiguity * 0.30)
            .min(1.0);
        let blast_radius_pressure =
            (centrality * 0.45 + cross_ratio * 0.35 + change_diffusion * 0.20).min(1.0);
        let history_support = file.revisions_window as f64 / (file.revisions_window as f64 + 5.0);
        let age_support = (file.age_days as f64 / 90.0).min(1.0);
        let ownership_concentration_pressure = if file.author_count_window == 0 {
            0.0
        } else {
            file.top_author_share * history_support * age_support
        };
        let coordination_authorship_pressure = if file.author_count_window == 0 {
            0.0
        } else {
            ((file.author_entropy / 4.0).min(1.0) * 0.6
                + (file.author_count_window as f64 / 10.0).min(1.0) * 0.4)
                .min(1.0)
        };
        let stale_ownership_pressure = file
            .days_since_non_bot_edit
            .map(|days| (days as f64 / 365.0).min(1.0) * history_support)
            .unwrap_or(0.0);
        let stewardship_pressure = if file.author_count_window == 0 {
            0.0
        } else {
            (ownership_concentration_pressure * 0.4
                + coordination_authorship_pressure * 0.35
                + stale_ownership_pressure * 0.25)
                .min(1.0)
        };
        let mut distinctive_terms = file
            .top_structural_terms
            .iter()
            .filter(|term| !language_common_term(term))
            .cloned()
            .collect::<Vec<_>>();
        distinctive_terms.sort_by(|left, right| {
            term_documents
                .get(left)
                .cmp(&term_documents.get(right))
                .then_with(|| left.cmp(right))
        });
        let navigation_term_limit =
            pointer_u64(config, "/navigation/top_distinctive_terms", 5) as usize;
        distinctive_terms.truncate(navigation_term_limit);
        let max_common_documents = (file_count / 4).max(2);
        let mut drift_terms: Vec<String> = file
            .top_structural_terms
            .iter()
            .filter(|term| !language_common_term(term))
            .filter(|term| term_roots.get(*term).is_some_and(|roots| roots.len() > 1))
            .filter(|term| {
                term_documents.get(*term).copied().unwrap_or_default() <= max_common_documents
            })
            .cloned()
            .collect();
        drift_terms.sort();
        let semantic_term_limit =
            pointer_u64(config, "/semantic_drift/top_term_limit", 25) as usize;
        drift_terms.truncate(semantic_term_limit);
        let idf = |term: &str| {
            let documents = term_documents.get(term).copied().unwrap_or_default() as f64;
            ((file_count as f64 + 1.0) / (documents + 1.0)).ln() + 1.0
        };
        let total_term_weight = file
            .top_structural_terms
            .iter()
            .filter(|term| !language_common_term(term))
            .map(|term| idf(term))
            .sum::<f64>();
        let drift_term_weight = drift_terms.iter().map(|term| idf(term)).sum::<f64>();
        let semantic_drift_pressure = if total_term_weight == 0.0 {
            0.0
        } else {
            (drift_term_weight / total_term_weight).min(1.0)
        };
        let drift_term_count = drift_terms.len();

        let related_relationship_ids = relationship_ids
            .get(&file.path)
            .cloned()
            .unwrap_or_default();
        let top_duplicate_relationship_ids: Vec<String> = related_relationship_ids
            .iter()
            .filter(|id| {
                id.starts_with("duplicate_neighborhood-")
                    || id.starts_with("near_duplicate_neighborhood-")
            })
            .take(5)
            .cloned()
            .collect();
        let top_coupling_relationship_ids: Vec<String> = related_relationship_ids
            .iter()
            .filter(|id| id.starts_with("temporal_coupling_edge-"))
            .take(5)
            .cloned()
            .collect();
        let organization_overlay = json!({
            "path": file.path,
            "duplication_pressure": round6(duplication_pressure),
            "diffusion_pressure": round6(diffusion_pressure),
            "coupling_pressure": round6(coupling_pressure),
            "boundary_pressure": round6(boundary_pressure),
            "duplicate_token_ratio": round6(duplication_pressure),
            "high_diffusion_commit_count": if average_files >= diffusion_p95 { commit_count } else { 0 },
            "cross_boundary_edge_count": cross_edges,
            "top_duplicate_relationship_ids": top_duplicate_relationship_ids,
            "top_coupling_relationship_ids": top_coupling_relationship_ids,
            "relationship_ids": related_relationship_ids,
            "cluster_ids": cluster_ids.get(&file.path).cloned().unwrap_or_default()
        });
        let folder = immediate_parent(&file.path);
        let folder_token_count = folder_tokens.get(&folder).copied().unwrap_or(file.tokens);
        let top_3_file_tokens: usize = folder_children
            .get(&folder)
            .map(|tokens| tokens.iter().take(3).sum())
            .unwrap_or(file.tokens);
        file.costs = json!({
            "load": {
                "file_token_count": file.tokens,
                "folder_token_count": folder_token_count,
                "top_file_share": round6(file.tokens as f64 / folder_token_count.max(1) as f64),
                "top_3_file_share": round6(top_3_file_tokens as f64 / folder_token_count.max(1) as f64),
                "token_concentration_ratio": round6(file.tokens as f64 / total_tokens as f64),
                "context_band": file.context_band,
                "load_pressure": round6(file.context_pressure)
            },
            "volatility": {
                "commit_count_window": file.revisions_window,
                "recency_weighted_commits": round6(file.recency_weighted_commits),
                "line_churn_window": file.churn_lines_window,
                "token_churn_window": file.token_churn_window,
                "relative_token_churn": round6(file.token_churn_window as f64 / file.tokens.max(1) as f64),
                "churn_measurement": "measured_numstat",
                "late_churn_spike": round6(file.late_churn_spike),
                "volatility_pressure": round6(file.churn_pressure)
            },
            "coordination": {
                "files_touched_per_change": round6(average_files),
                "folders_touched_per_change": round6(average_folders),
                "edit_hunks_per_change": round6(average_hunks),
                "cochange_degree": degree,
                "cochange_centrality": round6(centrality),
                "cross_folder_cochange_ratio": round6(cross_ratio),
                "change_diffusion": round6(change_diffusion),
                "coordination_pressure": round6(coordination_pressure),
                "cochange_pagerank": round6(cochange_pagerank)
            }
        });
        file.overlays = json!({
            "organization_health": organization_overlay,
            "verification": {
                "path": file.path,
                "applicability": verification_applicability,
                "evidence_status": if verification_applicability == "applicable" { "measured" } else { "not_applicable" },
                "test_adjacency_score": round6(test_adjacency),
                "inline_tests_detected": file.has_inline_tests,
                "nearby_test_paths": nearby_tests,
                "test_cochange_ratio": round6(test_cochange_ratio),
                "hotspot_without_nearby_tests": hotspot_without_nearby_tests,
                "churn_without_test_churn": file.churn_pressure >= 0.6 && test_cochange_ratio == 0.0,
                "verification_gap": verification_gap.map(round6)
            },
            "navigation": {
                "path": file.path,
                "path_depth": path_depth,
                "sibling_count": sibling_count,
                "folder_width": sibling_count,
                "search_ambiguity": round6(search_ambiguity),
                "term_dispersion": round6(semantic_drift_pressure),
                "top_distinctive_terms": distinctive_terms,
                "duplicate_name_count": duplicate_name_count,
                "navigation_pressure": round6(navigation_pressure)
            },
            "blast_radius": {
                "path": file.path,
                "cochange_degree": degree,
                "weighted_cochange_degree": neighbors.map(|items| items.values().sum::<usize>()).unwrap_or_default(),
                "cochange_pagerank": round6(cochange_pagerank),
                "cross_folder_coupling": round6(cross_ratio),
                "average_changeset_size_when_touched": round6(average_files),
                "blast_radius_pressure": round6(blast_radius_pressure)
            },
            "stewardship": {
                "path": file.path,
                "author_count_window": file.author_count_window,
                "author_entropy": round6(file.author_entropy),
                "top_author_share": round6(file.top_author_share),
                "days_since_non_bot_edit": file.days_since_non_bot_edit,
                "recent_maintainer_diversity": file.recent_maintainer_diversity,
                "ownership_concentration_pressure": round6(ownership_concentration_pressure),
                "many_author_coordination_pressure": round6(coordination_authorship_pressure),
                "stale_ownership_pressure": round6(stale_ownership_pressure),
                "stewardship_pressure": round6(stewardship_pressure)
                ,"history_support": round6(history_support)
                ,"age_support": round6(age_support)
                ,"confidence": if file.revisions_window >= 5 && file.age_days >= 90 { "supported" } else { "low_support" }
            },
            "concept_dispersion": {
                "path": file.path,
                "dispersed_terms": drift_terms,
                "concept_dispersion_pressure": round6(semantic_drift_pressure),
                "supporting_term_count": drift_term_count,
                "confidence": if drift_term_count >= 3 { "supported" } else { "low_support" },
                "method": "cross-root-idf-v3",
                "interpretation": "Cross-root concept dispersion; this does not measure temporal semantic drift."
            }
        });
        let mut structural = file
            .overlays
            .get("organization_health")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if let Some(item) = structural.as_object_mut() {
            item.insert("path".into(), json!(file.path));
        }
        top_structural_files.push(structural.clone());
        file_overlays.insert(file.path.clone(), structural);
    }
    top_structural_files.sort_by(|left, right| {
        let pressure = |item: &Value| {
            item["duplication_pressure"].as_f64().unwrap_or_default()
                + item["diffusion_pressure"].as_f64().unwrap_or_default()
                + item["coupling_pressure"].as_f64().unwrap_or_default()
                + item["boundary_pressure"].as_f64().unwrap_or_default()
        };
        pressure(right)
            .total_cmp(&pressure(left))
            .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
    });
    top_structural_files.truncate(10);
    let folder_overlays = folder_overlay_map(files);
    let organization_metrics = json!({
        "analysis_status": ORGANIZATION_ANALYSIS_STATUS,
        "analysis_version": ORGANIZATION_ANALYSIS_VERSION,
        "repo_baselines": {
            "file_count": files.len(),
            "diffusion_p95": round6(diffusion_p95)
        },
        "files": file_overlays.values().cloned().collect::<Vec<_>>(),
        "folders": folder_overlays.values().cloned().collect::<Vec<_>>()
    });
    Ok(OrganizationAnalysis {
        organization_metrics,
        relationships,
        clusters,
        file_overlays,
        folder_overlays,
        top_structural_files,
    })
}
