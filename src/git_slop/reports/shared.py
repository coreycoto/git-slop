from __future__ import annotations

from pathlib import PurePosixPath
from typing import Any, Iterable

from git_slop.graphs import (
    clusters_for_path,
    folder_clusters_for_prefix,
    folder_relationships_for_prefix,
    relationships_for_path,
)
from git_slop.graphs.clusters import CLUSTER_KEYS
from git_slop.graphs.relationships import RELATIONSHIP_KEYS
from git_slop.reporting import build_show_payload

EXPECTED_REPORT_SCHEMA_VERSION = 3


def relationship_sections(report: dict[str, Any]) -> dict[str, Any]:
    return (
        report.get("overlays", {})
        .get("organization_health", {})
        .get("relationships", {})
        or report.get("relationships", {})
    )


def cluster_sections(report: dict[str, Any]) -> dict[str, Any]:
    return (
        report.get("overlays", {})
        .get("organization_health", {})
        .get("clusters", {})
        or report.get("clusters", {})
    )


def dedupe_by_id(items: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    seen: set[str] = set()
    deduped: list[dict[str, Any]] = []
    for item in items:
        item_id = item.get("id")
        if not isinstance(item_id, str) or item_id in seen:
            continue
        seen.add(item_id)
        deduped.append(item)
    return deduped


def unique_preserving_order(values: Iterable[str]) -> list[str]:
    seen: set[str] = set()
    ordered: list[str] = []
    for value in values:
        if value in seen:
            continue
        seen.add(value)
        ordered.append(value)
    return ordered


def iter_relationships(report: dict[str, Any]) -> list[dict[str, Any]]:
    sections = relationship_sections(report)
    relationships: list[dict[str, Any]] = []
    for key in RELATIONSHIP_KEYS:
        relationships.extend(sections.get(key, []))
    ordered = sorted(
        relationships,
        key=lambda item: (-item["evidence_score"], item["id"]),
    )
    return dedupe_by_id(ordered)


def iter_clusters(report: dict[str, Any]) -> list[dict[str, Any]]:
    sections = cluster_sections(report)
    clusters: list[dict[str, Any]] = []
    for key in CLUSTER_KEYS:
        clusters.extend(sections.get(key, []))
    return dedupe_by_id(sorted(clusters, key=lambda item: (-item["evidence_score"], item["id"])))


def relationship_by_id(report: dict[str, Any], relationship_id: str) -> dict[str, Any] | None:
    for relationship in iter_relationships(report):
        if relationship["id"] == relationship_id:
            return relationship
    return None


def cluster_by_id(report: dict[str, Any], cluster_id: str) -> dict[str, Any] | None:
    for cluster in iter_clusters(report):
        if cluster["id"] == cluster_id:
            return cluster
    return None


def resolve_path(report: dict[str, Any], target_path: str) -> dict[str, Any] | None:
    normalized = target_path.strip() or "."
    record = build_show_payload(report, normalized)
    if record is None and normalized != ".":
        record = build_show_payload(report, normalized.rstrip("/"))
    return record


def record_summary(record: dict[str, Any] | None) -> dict[str, Any] | None:
    if record is None:
        return None
    summary: dict[str, Any] = {
        "path": record["path"],
        "priority_score": record.get("priority_score"),
        "priority_band": record.get("priority_band"),
        "context_band": record.get("context_band"),
        "reason_codes": record.get("reason_codes", []),
    }
    if "costs" in record:
        summary["costs"] = record["costs"]
    if "overlays" in record:
        summary["overlays"] = record["overlays"]
    return summary


def all_relationships_for_record(
    report: dict[str, Any],
    record: dict[str, Any],
) -> list[dict[str, Any]]:
    if record["record_type"] == "folder":
        return folder_relationships_for_prefix(report, record["path"])
    return relationships_for_path(report, record["path"])


def all_clusters_for_record(report: dict[str, Any], record: dict[str, Any]) -> list[dict[str, Any]]:
    if record["record_type"] == "folder":
        return folder_clusters_for_prefix(report, record["path"])
    return clusters_for_path(report, record["path"])


def descendant_file_records(report: dict[str, Any], folder_path: str) -> list[dict[str, Any]]:
    prefix = "" if folder_path == "." else f"{folder_path.rstrip('/')}/"
    records = [
        record
        for record in report.get("files", [])
        if folder_path == "." or record["path"].startswith(prefix)
    ]
    return sorted(
        records,
        key=lambda item: (-float(item.get("priority_score", 0.0)), item["path"]),
    )


def descendant_hotspots(
    report: dict[str, Any],
    folder_path: str,
    *,
    limit: int | None = None,
) -> list[dict[str, Any]]:
    prefix = "" if folder_path == "." else f"{folder_path.rstrip('/')}/"
    matched = [
        item
        for item in report.get("action_queue", [])
        if folder_path == "." or item["path"].startswith(prefix)
    ]
    return matched[:limit] if limit is not None else matched


def descendant_overlay_maxima(records: list[dict[str, Any]]) -> dict[str, Any]:
    if not records:
        return {}
    def _overlay_float(record: dict[str, Any], overlay_name: str, key: str) -> float:
        overlay = record.get("overlays", {}).get(overlay_name) or {}
        return float(overlay.get(key, 0.0))

    return {
        "organization_health": {
            "duplication_pressure": round(
                max(
                    _overlay_float(record, "organization_health", "duplication_pressure")
                    for record in records
                ),
                6,
            ),
            "diffusion_pressure": round(
                max(
                    _overlay_float(record, "organization_health", "diffusion_pressure")
                    for record in records
                ),
                6,
            ),
            "coupling_pressure": round(
                max(
                    _overlay_float(record, "organization_health", "coupling_pressure")
                    for record in records
                ),
                6,
            ),
            "boundary_pressure": round(
                max(
                    _overlay_float(record, "organization_health", "boundary_pressure")
                    for record in records
                ),
                6,
            ),
        },
        "verification": {
            "verification_gap": round(
                max(
                    _overlay_float(record, "verification", "verification_gap")
                    for record in records
                ),
                6,
            ),
        },
        "navigation": {
            "navigation_pressure": round(
                max(
                    _overlay_float(record, "navigation", "navigation_pressure")
                    for record in records
                ),
                6,
            ),
        },
        "blast_radius": {
            "blast_radius_pressure": round(
                max(
                    _overlay_float(record, "blast_radius", "blast_radius_pressure")
                    for record in records
                ),
                6,
            )
        },
        "stewardship": {
            "stewardship_pressure": round(
                max(
                    _overlay_float(record, "stewardship", "stewardship_pressure")
                    for record in records
                ),
                6,
            )
        },
        "semantic_drift": {
            "semantic_drift_pressure": round(
                max(
                    _overlay_float(record, "semantic_drift", "semantic_drift_pressure")
                    for record in records
                ),
                6,
            )
        },
    }


def strongest_pressures(overlays: dict[str, Any], *, limit: int = 3) -> list[tuple[str, float]]:
    organization_health = overlays.get("organization_health", {})
    organization_candidates = [
        ("organization.duplication", float(organization_health.get("duplication_pressure", 0.0))),
        ("organization.diffusion", float(organization_health.get("diffusion_pressure", 0.0))),
        ("organization.coupling", float(organization_health.get("coupling_pressure", 0.0))),
        ("organization.boundary", float(organization_health.get("boundary_pressure", 0.0))),
    ]
    candidates = organization_candidates + [
        ("verification", float(overlays.get("verification", {}).get("verification_gap", 0.0))),
        ("navigation", float(overlays.get("navigation", {}).get("navigation_pressure", 0.0))),
        (
            "blast_radius",
            float(overlays.get("blast_radius", {}).get("blast_radius_pressure", 0.0)),
        ),
        (
            "stewardship",
            float(overlays.get("stewardship", {}).get("stewardship_pressure", 0.0)),
        ),
        (
            "semantic_drift",
            float(overlays.get("semantic_drift", {}).get("semantic_drift_pressure", 0.0)),
        ),
    ]
    strongest = sorted(candidates, key=lambda item: (-item[1], item[0]))
    strongest = [item for item in strongest if item[1] > 0.0]
    if strongest:
        return strongest[:limit]
    return sorted(candidates, key=lambda item: item[0])[:1]


def path_matches_folder(path: str, folder_path: str) -> bool:
    if folder_path == ".":
        return True
    prefix = f"{folder_path.rstrip('/')}/"
    return path.startswith(prefix)


def top_level_root(path: str) -> str:
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


def relationship_focus_count(
    relationship: dict[str, Any],
    *,
    anchor_paths: list[str],
    focus_folder: str | None = None,
) -> tuple[int, int]:
    endpoints = [relationship["source_path"], relationship["target_path"]]
    anchor_matches = sum(path in anchor_paths for path in endpoints)
    if focus_folder is None:
        return anchor_matches, anchor_matches
    folder_matches = sum(path_matches_folder(path, focus_folder) for path in endpoints)
    return folder_matches, anchor_matches


def rank_relationships(
    relationships: Iterable[dict[str, Any]],
    *,
    anchor_paths: list[str],
    focus_folder: str | None = None,
) -> list[dict[str, Any]]:
    deduped = dedupe_by_id(relationships)
    ranked: list[dict[str, Any]] = []
    for relationship in deduped:
        folder_matches, anchor_matches = relationship_focus_count(
            relationship,
            anchor_paths=anchor_paths,
            focus_folder=focus_folder,
        )
        if focus_folder is not None and folder_matches == 0:
            continue
        ranked.append(relationship)
    return sorted(
        ranked,
        key=lambda item: (
            -relationship_focus_count(
                item,
                anchor_paths=anchor_paths,
                focus_folder=focus_folder,
            )[0],
            -relationship_focus_count(
                item,
                anchor_paths=anchor_paths,
                focus_folder=focus_folder,
            )[1],
            -float(item.get("evidence_score", 0.0)),
            item["id"],
        ),
    )


def cluster_focus_count(
    cluster: dict[str, Any],
    *,
    anchor_paths: list[str],
    focus_folder: str | None = None,
) -> tuple[int, int]:
    member_paths = cluster.get("member_paths", [])
    anchor_matches = sum(path in anchor_paths for path in member_paths)
    if focus_folder is None:
        return anchor_matches, anchor_matches
    folder_matches = sum(path_matches_folder(path, focus_folder) for path in member_paths)
    return folder_matches, anchor_matches


def rank_clusters(
    clusters: Iterable[dict[str, Any]],
    *,
    anchor_paths: list[str],
    focus_folder: str | None = None,
) -> list[dict[str, Any]]:
    deduped = dedupe_by_id(clusters)
    ranked: list[dict[str, Any]] = []
    for cluster in deduped:
        folder_matches, anchor_matches = cluster_focus_count(
            cluster,
            anchor_paths=anchor_paths,
            focus_folder=focus_folder,
        )
        if focus_folder is not None and folder_matches == 0:
            continue
        ranked.append(cluster)
    return sorted(
        ranked,
        key=lambda item: (
            -(
                cluster_focus_count(
                    item,
                    anchor_paths=anchor_paths,
                    focus_folder=focus_folder,
                )[0]
                / max(item.get("member_count", len(item.get("member_paths", []))), 1)
            ),
            -cluster_focus_count(
                item,
                anchor_paths=anchor_paths,
                focus_folder=focus_folder,
            )[1],
            item.get("member_count", len(item.get("member_paths", []))),
            -cluster_focus_count(
                item,
                anchor_paths=anchor_paths,
                focus_folder=focus_folder,
            )[0],
            len(item.get("top_level_roots", [])),
            -float(item.get("evidence_score", 0.0)),
            item["id"],
        ),
    )


def shared_clusters_for_relationship(
    report: dict[str, Any],
    relationship: dict[str, Any],
) -> list[dict[str, Any]]:
    shared = [
        cluster
        for cluster in iter_clusters(report)
        if relationship["source_path"] in cluster["member_paths"]
        and relationship["target_path"] in cluster["member_paths"]
    ]
    return rank_clusters(
        shared,
        anchor_paths=[relationship["source_path"], relationship["target_path"]],
    )
