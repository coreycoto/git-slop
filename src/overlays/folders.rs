use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::common::round6;
use crate::model::{FileAnalysis, parent_folders};

pub(super) fn folder_overlay_map(files: &[FileAnalysis]) -> BTreeMap<String, Value> {
    let mut groups: BTreeMap<String, Vec<&FileAnalysis>> = BTreeMap::new();
    for file in files {
        for folder in parent_folders(&file.path) {
            groups.entry(folder).or_default().push(file);
        }
    }
    groups
        .into_iter()
        .map(|(path, descendants)| {
            let maximum = |pointer: &str| {
                descendants
                    .iter()
                    .filter_map(|file| file.overlays.pointer(pointer).and_then(Value::as_f64))
                    .fold(0.0, f64::max)
            };
            let sum_u64 = |pointer: &str| {
                descendants
                    .iter()
                    .filter_map(|file| file.overlays.pointer(pointer).and_then(Value::as_u64))
                    .sum::<u64>()
            };
            let collect_ids = |pointer: &str, limit: Option<usize>| {
                let mut ids = BTreeSet::new();
                for file in &descendants {
                    if let Some(values) = file.overlays.pointer(pointer).and_then(Value::as_array) {
                        ids.extend(values.iter().filter_map(Value::as_str).map(ToOwned::to_owned));
                    }
                }
                ids.into_iter()
                    .take(limit.unwrap_or(usize::MAX))
                    .collect::<Vec<_>>()
            };
            let top_file_path = descendants
                .iter()
                .max_by(|left, right| {
                    let pressure = |file: &&FileAnalysis| {
                        [
                            "/organization_health/duplication_pressure",
                            "/organization_health/diffusion_pressure",
                            "/organization_health/coupling_pressure",
                            "/organization_health/boundary_pressure",
                        ]
                        .into_iter()
                        .filter_map(|pointer| {
                            file.overlays.pointer(pointer).and_then(Value::as_f64)
                        })
                        .fold(0.0, f64::max)
                    };
                    pressure(left)
                        .total_cmp(&pressure(right))
                        .then_with(|| right.path.cmp(&left.path))
                })
                .map(|file| file.path.as_str())
                .unwrap_or_default();
            (
                path.clone(),
                json!({
                    "path": path,
                    "descendant_file_count": descendants.len(),
                    "duplication_pressure": round6(maximum("/organization_health/duplication_pressure")),
                    "diffusion_pressure": round6(maximum("/organization_health/diffusion_pressure")),
                    "coupling_pressure": round6(maximum("/organization_health/coupling_pressure")),
                    "boundary_pressure": round6(maximum("/organization_health/boundary_pressure")),
                    "top_duplicate_relationship_ids": collect_ids(
                        "/organization_health/top_duplicate_relationship_ids",
                        Some(5),
                    ),
                    "top_coupling_relationship_ids": collect_ids(
                        "/organization_health/top_coupling_relationship_ids",
                        Some(5),
                    ),
                    "cluster_ids": collect_ids("/organization_health/cluster_ids", None),
                    "duplicate_token_ratio": round6(maximum("/organization_health/duplicate_token_ratio")),
                    "high_diffusion_commit_count": sum_u64(
                        "/organization_health/high_diffusion_commit_count",
                    ),
                    "cross_boundary_edge_count": sum_u64(
                        "/organization_health/cross_boundary_edge_count",
                    ),
                    "top_file_path": top_file_path
                }),
            )
        })
        .collect()
}
