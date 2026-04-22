from __future__ import annotations

from typing import Any

CLUSTER_KEYS = (
    "duplicate_sets",
    "scattered_concepts",
    "boundary_leakage_clusters",
    "consolidation_candidates",
)


def clusters_for_path(report: dict[str, Any], target_path: str) -> list[dict[str, Any]]:
    matched: list[dict[str, Any]] = []
    cluster_sections = (
        report.get("clusters")
        or report.get("overlays", {})
        .get("organization_health", {})
        .get("clusters", {})
    )
    for key in CLUSTER_KEYS:
        for cluster in cluster_sections.get(key, []):
            if target_path in cluster["member_paths"]:
                matched.append(cluster)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))


def folder_clusters_for_prefix(report: dict[str, Any], folder_path: str) -> list[dict[str, Any]]:
    prefix = "" if folder_path == "." else f"{folder_path.rstrip('/')}/"
    matched: list[dict[str, Any]] = []
    cluster_sections = (
        report.get("clusters")
        or report.get("overlays", {})
        .get("organization_health", {})
        .get("clusters", {})
    )
    for key in CLUSTER_KEYS:
        for cluster in cluster_sections.get(key, []):
            if any(path.startswith(prefix) for path in cluster["member_paths"]):
                matched.append(cluster)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))
