from __future__ import annotations

from typing import Any

RELATIONSHIP_KEYS = (
    "duplicate_neighborhoods",
    "near_duplicate_neighborhoods",
    "temporal_coupling_edges",
    "lexical_affinity_edges",
    "boundary_leakage_edges",
)


def relationships_for_path(report: dict[str, Any], target_path: str) -> list[dict[str, Any]]:
    matched: list[dict[str, Any]] = []
    relationship_sections = (
        report.get("relationships")
        or report.get("overlays", {})
        .get("organization_health", {})
        .get("relationships", {})
    )
    for key in RELATIONSHIP_KEYS:
        for relationship in relationship_sections.get(key, []):
            if (
                relationship["source_path"] == target_path
                or relationship["target_path"] == target_path
            ):
                matched.append(relationship)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))


def folder_relationships_for_prefix(
    report: dict[str, Any],
    folder_path: str,
) -> list[dict[str, Any]]:
    prefix = "" if folder_path == "." else f"{folder_path.rstrip('/')}/"
    matched: list[dict[str, Any]] = []
    relationship_sections = (
        report.get("relationships")
        or report.get("overlays", {})
        .get("organization_health", {})
        .get("relationships", {})
    )
    for key in RELATIONSHIP_KEYS:
        for relationship in relationship_sections.get(key, []):
            if (
                relationship["source_path"].startswith(prefix)
                or relationship["target_path"].startswith(prefix)
            ):
                matched.append(relationship)
    return sorted(matched, key=lambda item: (-item["evidence_score"], item["id"]))
