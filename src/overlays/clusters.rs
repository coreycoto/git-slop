use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::common::{
    ORGANIZATION_ANALYSIS_STATUS, ORGANIZATION_ANALYSIS_VERSION, round6, stable_id,
};

pub(super) fn build_clusters(
    duplicate_relationships: &[Value],
) -> (Value, BTreeMap<String, Vec<String>>) {
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut relation_by_pair: BTreeMap<(String, String), String> = BTreeMap::new();
    for relationship in duplicate_relationships {
        let Some(source) = relationship["source_path"].as_str() else {
            continue;
        };
        let Some(target) = relationship["target_path"].as_str() else {
            continue;
        };
        adjacency
            .entry(source.to_string())
            .or_default()
            .insert(target.to_string());
        adjacency
            .entry(target.to_string())
            .or_default()
            .insert(source.to_string());
        relation_by_pair.insert(
            (source.to_string(), target.to_string()),
            relationship["id"].as_str().unwrap_or_default().to_string(),
        );
    }
    let mut visited = BTreeSet::new();
    let mut duplicate_sets = Vec::new();
    let mut candidates = Vec::new();
    let mut memberships: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for start in adjacency.keys() {
        if !visited.insert(start.clone()) {
            continue;
        }
        let mut stack = vec![start.clone()];
        let mut members = Vec::new();
        while let Some(path) = stack.pop() {
            members.push(path.clone());
            if let Some(neighbors) = adjacency.get(&path) {
                for neighbor in neighbors.iter().rev() {
                    if visited.insert(neighbor.clone()) {
                        stack.push(neighbor.clone());
                    }
                }
            }
        }
        members.sort();
        if members.len() < 2 {
            continue;
        }
        let member_refs: Vec<&str> = members.iter().map(String::as_str).collect();
        let id = stable_id("duplicate_set", &member_refs);
        let source_relationship_ids: Vec<String> = relation_by_pair
            .iter()
            .filter(|((left, right), _)| members.contains(left) && members.contains(right))
            .map(|(_, id)| id.clone())
            .collect();
        let roots: BTreeSet<String> = members
            .iter()
            .map(|path| crate::model::top_level_root(path))
            .collect();
        for path in &members {
            memberships
                .entry(path.clone())
                .or_default()
                .push(id.clone());
        }
        let duplicate_set = json!({
            "id": id,
            "kind": "duplicate_set",
            "candidate_type": "consolidate_duplicate_knowledge",
            "member_count": members.len(),
            "member_paths": members,
            "top_level_roots": roots,
            "evidence_score": round6((source_relationship_ids.len() as f64 / 5.0).min(1.0)),
            "source_relationship_ids": source_relationship_ids
        });
        let mut candidate = duplicate_set.clone();
        if let Some(object) = candidate.as_object_mut() {
            object.insert("kind".to_string(), json!("consolidation_candidate"));
        }
        duplicate_sets.push(duplicate_set);
        candidates.push(candidate);
    }
    duplicate_sets.sort_by(|left, right| {
        right["evidence_score"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&left["evidence_score"].as_f64().unwrap_or_default())
            .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
    });
    candidates.sort_by(|left, right| {
        right["evidence_score"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&left["evidence_score"].as_f64().unwrap_or_default())
            .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
    });
    (
        json!({
            "analysis_status": ORGANIZATION_ANALYSIS_STATUS,
            "analysis_version": ORGANIZATION_ANALYSIS_VERSION,
            "duplicate_sets": duplicate_sets,
            "scattered_concepts": [],
            "boundary_leakage_clusters": [],
            "consolidation_candidates": candidates
        }),
        memberships,
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::build_clusters;
    use crate::overlays::common::{
        ORGANIZATION_ANALYSIS_STATUS, ORGANIZATION_ANALYSIS_VERSION, stable_id,
    };

    #[test]
    fn organization_sections_keep_canonical_metadata_keys_and_kinds() {
        let relationship_id = stable_id("duplicate_neighborhood", &["src/a.rs", "src/b.rs"]);
        let duplicate = json!({
            "id": relationship_id,
            "kind": "duplicate_neighborhood",
            "source_path": "src/a.rs",
            "target_path": "src/b.rs",
            "evidence_score": 1.0
        });
        let (clusters, _) = build_clusters(&[duplicate]);

        assert_eq!(clusters["analysis_status"], ORGANIZATION_ANALYSIS_STATUS);
        assert_eq!(clusters["analysis_version"], ORGANIZATION_ANALYSIS_VERSION);
        for key in [
            "duplicate_sets",
            "scattered_concepts",
            "boundary_leakage_clusters",
            "consolidation_candidates",
        ] {
            assert!(clusters[key].is_array(), "missing canonical array {key}");
        }
        assert_eq!(clusters["duplicate_sets"][0]["kind"], "duplicate_set");
        assert_eq!(
            clusters["consolidation_candidates"][0]["kind"],
            "consolidation_candidate"
        );
        assert_eq!(
            clusters["duplicate_sets"][0]["id"],
            clusters["consolidation_candidates"][0]["id"]
        );
    }
}
