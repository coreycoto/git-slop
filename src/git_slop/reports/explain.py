from __future__ import annotations

from typing import Any

from git_slop.graphs.clusters import CLUSTER_KEYS
from git_slop.graphs.relationships import RELATIONSHIP_KEYS
from git_slop.reporting import build_show_payload

EXPLAIN_SCHEMA_VERSION = 1
BOUNDARY_NOTE = (
    "Interpretation boundary: this is structural evidence, not proof that an "
    "abstraction, boundary, or refactor is correct."
)


def _relationship_sections(report: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    canonical = report.get("overlays", {}).get("organization_health", {}).get("relationships", {})
    return canonical or report.get("relationships", {})


def _cluster_sections(report: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    canonical = report.get("overlays", {}).get("organization_health", {}).get("clusters", {})
    return canonical or report.get("clusters", {})


def _iter_relationships(report: dict[str, Any]) -> list[dict[str, Any]]:
    sections = _relationship_sections(report)
    relationships: list[dict[str, Any]] = []
    for key in RELATIONSHIP_KEYS:
        relationships.extend(sections.get(key, []))
    return sorted(relationships, key=lambda item: (-item["evidence_score"], item["id"]))


def _iter_clusters(report: dict[str, Any]) -> list[dict[str, Any]]:
    sections = _cluster_sections(report)
    clusters: list[dict[str, Any]] = []
    for key in CLUSTER_KEYS:
        clusters.extend(sections.get(key, []))
    return sorted(clusters, key=lambda item: (-item["evidence_score"], item["id"]))


def _relationship_by_id(report: dict[str, Any], relationship_id: str) -> dict[str, Any] | None:
    for relationship in _iter_relationships(report):
        if relationship["id"] == relationship_id:
            return relationship
    return None


def _cluster_by_id(report: dict[str, Any], cluster_id: str) -> dict[str, Any] | None:
    for cluster in _iter_clusters(report):
        if cluster["id"] == cluster_id:
            return cluster
    return None


def _resolve_path(report: dict[str, Any], target_path: str) -> dict[str, Any] | None:
    normalized = target_path.strip() or "."
    record = build_show_payload(report, normalized)
    if record is None and normalized != ".":
        record = build_show_payload(report, normalized.rstrip("/"))
    return record


def _record_summary(record: dict[str, Any] | None) -> dict[str, Any] | None:
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


def _member_overlay_maxima(member_records: list[dict[str, Any]]) -> dict[str, Any]:
    if not member_records:
        return {}
    return {
        "organization_health": {
            "duplication_pressure": round(
                max(
                    float(record["overlays"]["organization_health"]["duplication_pressure"])
                    for record in member_records
                ),
                6,
            ),
            "diffusion_pressure": round(
                max(
                    float(record["overlays"]["organization_health"]["diffusion_pressure"])
                    for record in member_records
                ),
                6,
            ),
            "coupling_pressure": round(
                max(
                    float(record["overlays"]["organization_health"]["coupling_pressure"])
                    for record in member_records
                ),
                6,
            ),
            "boundary_pressure": round(
                max(
                    float(record["overlays"]["organization_health"]["boundary_pressure"])
                    for record in member_records
                ),
                6,
            ),
        },
        "verification": {
            "verification_gap": round(
                max(
                    float(
                        record["overlays"]["verification"]["verification_gap"]
                    )
                    for record in member_records
                ),
                6,
            ),
        },
        "navigation": {
            "navigation_pressure": round(
                max(
                    float(
                        record["overlays"]["navigation"]["navigation_pressure"]
                    )
                    for record in member_records
                ),
                6,
            ),
        },
        "blast_radius": {
            "blast_radius_pressure": round(
                max(
                    float(
                        record["overlays"]["blast_radius"][
                            "blast_radius_pressure"
                        ]
                    )
                    for record in member_records
                ),
                6,
            ),
        },
        "stewardship": {
            "stewardship_pressure": round(
                max(
                    float(
                        record["overlays"]["stewardship"][
                            "stewardship_pressure"
                        ]
                    )
                    for record in member_records
                ),
                6,
            ),
        },
        "semantic_drift": {
            "semantic_drift_pressure": round(
                max(
                    float(record["overlays"]["semantic_drift"]["semantic_drift_pressure"])
                    for record in member_records
                ),
                6,
            ),
        },
    }


def _base_payload(
    report: dict[str, Any], selector: dict[str, Any], target: dict[str, Any]
) -> dict[str, Any]:
    return {
        "schema_version": EXPLAIN_SCHEMA_VERSION,
        "report_schema_version": report.get("schema_version"),
        "command": "explain",
        "selector": selector,
        "target": target,
        "boundary_note": BOUNDARY_NOTE,
    }


def _build_path_payload(report: dict[str, Any], target_path: str) -> dict[str, Any]:
    record = _resolve_path(report, target_path)
    if record is None:
        raise ValueError(f"No record found for '{target_path}'.")
    payload = _base_payload(
        report,
        selector={"kind": "path", "value": target_path},
        target={
            "kind": "path",
            "path": record["path"],
            "record_type": record["record_type"],
            "priority_score": record.get("priority_score"),
            "priority_band": record.get("priority_band"),
            "context_band": record.get("context_band"),
            "reason_codes": record.get("reason_codes", []),
        },
    )
    payload["cost_summary"] = record.get("costs", {})
    payload["overlay_summary"] = record.get("overlays", {})
    payload["supporting_relationships"] = record.get("strongest_relationships", [])[:5]
    payload["supporting_clusters"] = record.get("cluster_memberships", [])[:5]
    return payload


def _build_cluster_payload(report: dict[str, Any], cluster_id: str) -> dict[str, Any]:
    cluster = _cluster_by_id(report, cluster_id)
    if cluster is None:
        raise ValueError(f"No cluster found for '{cluster_id}'.")
    member_records = [
        record
        for record in (
            _resolve_path(report, member_path) for member_path in cluster["member_paths"]
        )
        if record is not None
    ]
    member_records = sorted(
        member_records,
        key=lambda item: (
            -(float(item.get("priority_score") or 0.0)),
            item["path"],
        ),
    )
    relationship_index = {item["id"]: item for item in _iter_relationships(report)}
    supporting_relationships = [
        relationship_index[relationship_id]
        for relationship_id in cluster.get("source_relationship_ids", [])
        if relationship_id in relationship_index
    ][:5]
    payload = _base_payload(
        report,
        selector={"kind": "cluster", "value": cluster_id},
        target={
            "kind": "cluster",
            "id": cluster["id"],
            "cluster_kind": cluster["kind"],
            "candidate_type": cluster.get("candidate_type"),
            "member_count": cluster["member_count"],
            "member_paths": cluster["member_paths"],
            "top_level_roots": cluster.get("top_level_roots", []),
        },
    )
    payload["cost_summary"] = {
        "member_hotspots": [_record_summary(record) for record in member_records[:5]],
        "member_count": cluster["member_count"],
        "top_level_roots": cluster.get("top_level_roots", []),
    }
    payload["overlay_summary"] = {
        "organization_health": cluster,
        "member_overlay_maxima": _member_overlay_maxima(member_records),
    }
    payload["supporting_relationships"] = supporting_relationships
    payload["supporting_clusters"] = [cluster]
    return payload


def _build_relationship_payload(report: dict[str, Any], relationship_id: str) -> dict[str, Any]:
    relationship = _relationship_by_id(report, relationship_id)
    if relationship is None:
        raise ValueError(f"No relationship found for '{relationship_id}'.")
    source_record = _resolve_path(report, relationship["source_path"])
    target_record = _resolve_path(report, relationship["target_path"])
    shared_clusters = [
        cluster
        for cluster in _iter_clusters(report)
        if relationship["source_path"] in cluster["member_paths"]
        and relationship["target_path"] in cluster["member_paths"]
    ][:5]
    payload = _base_payload(
        report,
        selector={"kind": "relationship", "value": relationship_id},
        target={
            "kind": "relationship",
            "id": relationship["id"],
            "relationship_kind": relationship["kind"],
            "source_path": relationship["source_path"],
            "target_path": relationship["target_path"],
            "evidence_score": relationship["evidence_score"],
        },
    )
    payload["cost_summary"] = {
        "source": _record_summary(source_record),
        "target": _record_summary(target_record),
    }
    payload["overlay_summary"] = {
        "organization_health": relationship,
        "source_overlays": source_record.get("overlays", {}) if source_record else {},
        "target_overlays": target_record.get("overlays", {}) if target_record else {},
    }
    payload["supporting_relationships"] = [relationship]
    payload["supporting_clusters"] = shared_clusters
    return payload


def _build_top_payload(report: dict[str, Any], count: int) -> dict[str, Any]:
    if count <= 0:
        raise ValueError("--top must be greater than zero.")
    items = [
        _build_path_payload(report, item["path"])
        for item in report.get("action_queue", [])[:count]
    ]
    return {
        "schema_version": EXPLAIN_SCHEMA_VERSION,
        "report_schema_version": report.get("schema_version"),
        "command": "explain",
        "selector": {"kind": "top", "value": count},
        "target": {"kind": "top", "count": count},
        "items": items,
        "boundary_note": BOUNDARY_NOTE,
    }


def build_explain_payload(
    report: dict[str, Any],
    *,
    path: str | None = None,
    cluster_id: str | None = None,
    relationship_id: str | None = None,
    top: int | None = None,
) -> dict[str, Any]:
    selectors = [
        path is not None,
        cluster_id is not None,
        relationship_id is not None,
        top is not None,
    ]
    if sum(selectors) > 1:
        raise ValueError("Select exactly one of --path, --cluster, --relationship, or --top.")
    if path is not None:
        return _build_path_payload(report, path)
    if cluster_id is not None:
        return _build_cluster_payload(report, cluster_id)
    if relationship_id is not None:
        return _build_relationship_payload(report, relationship_id)
    return _build_top_payload(report, top if top is not None else 5)


def _format_reason_codes(reason_codes: list[str]) -> str:
    return ", ".join(reason_codes) if reason_codes else "none"


def _format_overlay_lines(overlays: dict[str, Any]) -> list[str]:
    if not overlays:
        return ["- none"]
    organization_health = overlays.get("organization_health", {})
    verification = overlays.get("verification", {})
    navigation = overlays.get("navigation", {})
    blast_radius = overlays.get("blast_radius", {})
    stewardship = overlays.get("stewardship", {})
    semantic_drift = overlays.get("semantic_drift", {})
    hotspot_without_nearby_tests = bool(
        verification.get("hotspot_without_nearby_tests", False)
    )
    return [
        (
            "- organization_health: "
            f"duplication={organization_health.get('duplication_pressure', 0.0):.3f}, "
            f"diffusion={organization_health.get('diffusion_pressure', 0.0):.3f}, "
            f"coupling={organization_health.get('coupling_pressure', 0.0):.3f}, "
            f"boundary={organization_health.get('boundary_pressure', 0.0):.3f}, "
            f"clusters={len(organization_health.get('cluster_ids', []))}"
        ),
        (
            "- verification: "
            f"gap={verification.get('verification_gap', 0.0):.3f}, "
            f"adjacency={verification.get('test_adjacency_score', 0.0):.3f}, "
            f"test_cochange={verification.get('test_cochange_ratio', 0.0):.3f}, "
            f"hotspot_without_nearby_tests={hotspot_without_nearby_tests}"
        ),
        (
            "- navigation: "
            f"pressure={navigation.get('navigation_pressure', 0.0):.3f}, "
            f"ambiguity={navigation.get('search_ambiguity', 0.0):.3f}, "
            f"path_depth={navigation.get('path_depth', 0)}"
        ),
        (
            "- blast_radius: "
            f"pressure={blast_radius.get('blast_radius_pressure', 0.0):.3f}, "
            f"degree={blast_radius.get('cochange_degree', 0)}, "
            f"cross_folder={blast_radius.get('cross_folder_coupling', 0.0):.3f}"
        ),
        (
            "- stewardship: "
            f"pressure={stewardship.get('stewardship_pressure', 0.0):.3f}, "
            f"authors={stewardship.get('author_count_window', 0)}, "
            f"top_author_share={stewardship.get('top_author_share', 0.0):.3f}"
        ),
        (
            "- semantic_drift: "
            f"pressure={semantic_drift.get('semantic_drift_pressure', 0.0):.3f}, "
            f"terms={', '.join(semantic_drift.get('drift_terms', [])[:5]) or 'none'}"
        ),
    ]


def _format_relationship_brief(relationship: dict[str, Any]) -> str:
    return (
        f"- {relationship['id']} [{relationship['kind']}] "
        f"{relationship['source_path']} -> {relationship['target_path']} "
        f"(score={relationship['evidence_score']:.3f})"
    )


def _format_cluster_brief(cluster: dict[str, Any]) -> str:
    candidate_type = cluster.get("candidate_type") or cluster.get("kind")
    return (
        f"- {cluster['id']} [{candidate_type}] "
        f"members={cluster['member_count']} "
        f"roots={', '.join(cluster.get('top_level_roots', [])) or 'none'}"
    )


def _render_path_entry(payload: dict[str, Any]) -> str:
    target = payload["target"]
    cost_summary = payload.get("cost_summary", {})
    lines = [
        f"Explain: path {target['path']} [{target['record_type']}]",
        "",
        "Hotspot Cost",
        (
            "- priority: "
            f"{target.get('priority_band')} ({target.get('priority_score')}) "
            f"context={target.get('context_band')} "
            f"reasons={_format_reason_codes(target.get('reason_codes', []))}"
        ),
        (
            "- load: "
            f"tokens={cost_summary.get('load', {}).get('file_token_count', 0)}, "
            f"folder_tokens={cost_summary.get('load', {}).get('folder_token_count', 0)}, "
            f"pressure={cost_summary.get('load', {}).get('load_pressure', 0.0):.3f}"
        ),
        (
            "- volatility: "
            f"commits={cost_summary.get('volatility', {}).get('commit_count_window', 0)}, "
            f"relative_token_churn="
            f"{cost_summary.get('volatility', {}).get('relative_token_churn', 0.0):.3f}, "
            f"pressure={cost_summary.get('volatility', {}).get('volatility_pressure', 0.0):.3f}"
        ),
        (
            "- coordination: "
            f"diffusion={cost_summary.get('coordination', {}).get('change_diffusion', 0.0):.3f}, "
            f"degree={cost_summary.get('coordination', {}).get('cochange_degree', 0)}, "
            f"pressure={cost_summary.get('coordination', {}).get('coordination_pressure', 0.0):.3f}"
        ),
        "",
        "Overlay Evidence",
        *_format_overlay_lines(payload.get("overlay_summary", {})),
        "",
        "Supporting Relationships",
    ]
    relationships = payload.get("supporting_relationships", [])
    lines.extend(
        [_format_relationship_brief(relationship) for relationship in relationships] or ["- none"]
    )
    lines.extend(["", "Supporting Clusters"])
    clusters = payload.get("supporting_clusters", [])
    lines.extend([_format_cluster_brief(cluster) for cluster in clusters] or ["- none"])
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)


def _render_cluster_entry(payload: dict[str, Any]) -> str:
    target = payload["target"]
    member_hotspots = payload.get("cost_summary", {}).get("member_hotspots", [])
    organization_health = payload["overlay_summary"]["organization_health"]
    lines = [
        f"Explain: cluster {target['id']} [{target['cluster_kind']}]",
        "",
        "Hotspot Cost",
        (
            f"- members={target['member_count']} "
            f"roots={', '.join(target.get('top_level_roots', [])) or 'none'} "
            f"candidate_type={target.get('candidate_type') or 'n/a'}"
        ),
        "- member hotspots:",
    ]
    lines.extend(
        [
            (
                f"  - {item['path']} priority={item.get('priority_band')} "
                f"score={item.get('priority_score')} context={item.get('context_band')}"
            )
            for item in member_hotspots
        ]
        or ["  - none"]
    )
    lines.extend(
        [
            "",
            "Overlay Evidence",
            (
                "- organization_health: "
                f"candidate_type="
                f"{organization_health.get('candidate_type') or organization_health.get('kind')}, "
                f"evidence_score={organization_health.get('evidence_score', 0.0):.3f}"
            ),
        ]
    )
    maxima = payload["overlay_summary"].get("member_overlay_maxima", {})
    if maxima:
        organization_maxima = maxima.get("organization_health", {})
        verification_maxima = maxima.get("verification", {})
        navigation_maxima = maxima.get("navigation", {})
        blast_radius_maxima = maxima.get("blast_radius", {})
        semantic_drift_maxima = maxima.get("semantic_drift", {})
        lines.extend(
            [
                (
                    "- member overlay maxima: "
                    f"duplication={organization_maxima.get('duplication_pressure', 0.0):.3f}, "
                    f"verification_gap={verification_maxima.get('verification_gap', 0.0):.3f}, "
                    f"navigation={navigation_maxima.get('navigation_pressure', 0.0):.3f}, "
                    f"blast_radius={blast_radius_maxima.get('blast_radius_pressure', 0.0):.3f}, "
                    "semantic_drift="
                    f"{semantic_drift_maxima.get('semantic_drift_pressure', 0.0):.3f}"
                )
            ]
        )
    lines.extend(["", "Supporting Relationships"])
    relationships = payload.get("supporting_relationships", [])
    lines.extend(
        [_format_relationship_brief(relationship) for relationship in relationships] or ["- none"]
    )
    lines.extend(["", "Supporting Clusters"])
    clusters = payload.get("supporting_clusters", [])
    lines.extend([_format_cluster_brief(cluster) for cluster in clusters] or ["- none"])
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)


def _render_relationship_entry(payload: dict[str, Any]) -> str:
    target = payload["target"]
    cost_summary = payload.get("cost_summary", {})
    organization_health = payload["overlay_summary"]["organization_health"]
    crosses_top_level_boundary = bool(
        organization_health.get("crosses_top_level_boundary", False)
    )
    lines = [
        f"Explain: relationship {target['id']} [{target['relationship_kind']}]",
        "",
        "Hotspot Cost",
        (
            f"- source={target['source_path']} "
            f"priority={cost_summary.get('source', {}).get('priority_band')} "
            f"score={cost_summary.get('source', {}).get('priority_score')}"
        ),
        (
            f"- target={target['target_path']} "
            f"priority={cost_summary.get('target', {}).get('priority_band')} "
            f"score={cost_summary.get('target', {}).get('priority_score')}"
        ),
        "",
        "Overlay Evidence",
        (
            "- organization_health: "
            f"evidence_score={organization_health.get('evidence_score', 0.0):.3f}, "
            f"crosses_top_level_boundary={crosses_top_level_boundary}"
        ),
        "- source overlays:",
        *_format_overlay_lines(payload["overlay_summary"].get("source_overlays", {})),
        "- target overlays:",
        *_format_overlay_lines(payload["overlay_summary"].get("target_overlays", {})),
        "",
        "Supporting Relationships",
        _format_relationship_brief(payload["supporting_relationships"][0]),
        "",
        "Supporting Clusters",
    ]
    clusters = payload.get("supporting_clusters", [])
    lines.extend([_format_cluster_brief(cluster) for cluster in clusters] or ["- none"])
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)


def render_explain_text(payload: dict[str, Any]) -> str:
    selector_kind = payload.get("selector", {}).get("kind")
    if selector_kind == "top":
        count = payload.get("target", {}).get("count", 0)
        blocks = [f"Explain: top {count} hotspots"]
        for item in payload.get("items", []):
            blocks.extend(["", _render_path_entry(item)])
        blocks.extend(["", payload["boundary_note"]])
        return "\n".join(blocks)
    if selector_kind == "cluster":
        return _render_cluster_entry(payload)
    if selector_kind == "relationship":
        return _render_relationship_entry(payload)
    return _render_path_entry(payload)
