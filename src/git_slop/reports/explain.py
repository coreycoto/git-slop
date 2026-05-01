from __future__ import annotations

from typing import Any

from git_slop.reports.shared import (
    all_clusters_for_record,
    all_relationships_for_record,
    cluster_by_id,
    dedupe_by_id,
    descendant_file_records,
    descendant_hotspots,
    descendant_overlay_maxima,
    iter_relationships,
    rank_clusters,
    rank_relationships,
    record_summary,
    relationship_by_id,
    resolve_path,
    shared_clusters_for_relationship,
    strongest_pressures,
)

EXPLAIN_SCHEMA_VERSION = 2
BOUNDARY_NOTE = (
    "Interpretation boundary: this is structural evidence, not proof that an "
    "abstraction, boundary, or refactor is correct."
)


def _limit_deduped(items: list[dict[str, Any]], *, limit: int) -> list[dict[str, Any]]:
    return dedupe_by_id(items)[:limit]


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


def _cost_evidence_summary(costs: dict[str, Any]) -> list[str]:
    summaries: list[tuple[float, str]] = []
    load = costs.get("load", {}) or {}
    volatility = costs.get("volatility", {}) or {}
    coordination = costs.get("coordination", {}) or {}
    summaries.extend(
        [
            (
                float(load.get("load_pressure", 0.0)),
                (
                    "load pressure "
                    f"{float(load.get('load_pressure', 0.0)):.3f} from "
                    f"{int(load.get('file_token_count', 0))} file tokens"
                ),
            ),
            (
                float(volatility.get("volatility_pressure", 0.0)),
                (
                    "volatility pressure "
                    f"{float(volatility.get('volatility_pressure', 0.0)):.3f} from "
                    f"{int(volatility.get('commit_count_window', 0))} commits"
                ),
            ),
            (
                float(coordination.get("coordination_pressure", 0.0)),
                (
                    "coordination pressure "
                    f"{float(coordination.get('coordination_pressure', 0.0)):.3f} from "
                    f"degree {int(coordination.get('cochange_degree', 0))}"
                ),
            ),
        ]
    )
    ranked = [text for value, text in sorted(summaries, key=lambda item: (-item[0], item[1]))]
    return ranked[:3]


def _overlay_evidence_summary(overlays: dict[str, Any]) -> list[str]:
    strongest = strongest_pressures(overlays or {}, limit=3)
    return [f"{label} pressure {value:.3f}" for label, value in strongest]


def _evidence_summary(
    payload: dict[str, Any],
    *,
    mode: str,
) -> dict[str, Any]:
    relationships = payload.get("supporting_relationships", [])
    clusters = payload.get("supporting_clusters", [])
    return {
        "detector_cost": _cost_evidence_summary(payload.get("cost_summary", {})),
        "strongest_overlays": _overlay_evidence_summary(payload.get("overlay_summary", {})),
        "supporting_evidence": {
            "relationship_ids": [item["id"] for item in relationships[:5]],
            "cluster_ids": [item["id"] for item in clusters[:5]],
        },
        "interpretation": (
            f"{mode} explanation is based on detector report evidence only; "
            "it does not prove correctness or require a refactor."
        ),
    }


def _descendant_hotspot_summaries(report: dict[str, Any], folder_path: str) -> list[dict[str, Any]]:
    hotspots = descendant_hotspots(report, folder_path, limit=5)
    if hotspots:
        summaries: list[dict[str, Any]] = []
        for item in hotspots:
            record = resolve_path(report, item["path"])
            if record is not None:
                summary = record_summary(record)
                if summary is not None:
                    summaries.append(summary)
        return summaries
    return [
        record_summary(record)
        for record in descendant_file_records(report, folder_path)[:5]
    ]


def _build_file_payload(
    report: dict[str, Any],
    *,
    target_path: str,
    record: dict[str, Any],
) -> dict[str, Any]:
    payload = _base_payload(
        report,
        selector={"kind": "path", "value": target_path},
        target={
            "kind": "path",
            "path": record["path"],
            "record_type": record["record_type"],
            "slop_score": record.get("slop_score"),
            "slop_band": record.get("slop_band"),
            "context_band": record.get("context_band"),
            "reason_codes": record.get("reason_codes", []),
        },
    )
    payload["cost_summary"] = record.get("costs", {})
    payload["overlay_summary"] = record.get("overlays", {})
    payload["supporting_relationships"] = _limit_deduped(
        rank_relationships(
            all_relationships_for_record(report, record),
            anchor_paths=[record["path"]],
        ),
        limit=5,
    )
    payload["supporting_clusters"] = _limit_deduped(
        rank_clusters(
            all_clusters_for_record(report, record),
            anchor_paths=[record["path"]],
        ),
        limit=5,
    )
    payload["evidence_summary"] = _evidence_summary(payload, mode="Path")
    return payload


def _build_folder_payload(
    report: dict[str, Any],
    *,
    target_path: str,
    record: dict[str, Any],
) -> dict[str, Any]:
    descendant_records = descendant_file_records(report, record["path"])
    payload = _base_payload(
        report,
        selector={"kind": "path", "value": target_path},
        target={
            "kind": "path",
            "path": record["path"],
            "record_type": record["record_type"],
            "slop_score": record.get("slop_score"),
            "slop_band": record.get("slop_band"),
            "context_band": record.get("context_band"),
            "reason_codes": record.get("reason_codes", []),
        },
    )
    payload["cost_summary"] = {
        **record.get("costs", {}),
        "descendant_hotspots": _descendant_hotspot_summaries(report, record["path"]),
    }
    payload["overlay_summary"] = {
        **record.get("overlays", {}),
        "descendant_overlay_maxima": descendant_overlay_maxima(descendant_records),
    }
    descendant_paths = [item["path"] for item in payload["cost_summary"]["descendant_hotspots"]]
    if not descendant_paths:
        descendant_paths = [item["path"] for item in descendant_records[:5]]
    payload["supporting_relationships"] = _limit_deduped(
        rank_relationships(
            all_relationships_for_record(report, record),
            anchor_paths=descendant_paths or [record["path"]],
            focus_folder=record["path"],
        ),
        limit=5,
    )
    payload["supporting_clusters"] = _limit_deduped(
        rank_clusters(
            all_clusters_for_record(report, record),
            anchor_paths=descendant_paths or [record["path"]],
            focus_folder=record["path"],
        ),
        limit=5,
    )
    payload["evidence_summary"] = _evidence_summary(payload, mode="Folder")
    return payload


def _build_path_payload(report: dict[str, Any], target_path: str) -> dict[str, Any]:
    record = resolve_path(report, target_path)
    if record is None:
        raise ValueError(f"No record found for '{target_path}'.")
    if record["record_type"] == "folder":
        return _build_folder_payload(report, target_path=target_path, record=record)
    return _build_file_payload(report, target_path=target_path, record=record)


def _build_cluster_payload(report: dict[str, Any], cluster_id: str) -> dict[str, Any]:
    cluster = cluster_by_id(report, cluster_id)
    if cluster is None:
        raise ValueError(f"No cluster found for '{cluster_id}'.")
    member_records = [
        record
        for record in (
            resolve_path(report, member_path) for member_path in cluster["member_paths"]
        )
        if record is not None
    ]
    member_records = sorted(
        member_records,
        key=lambda item: (
            -(float(item.get("slop_score") or 0.0)),
            item["path"],
        ),
    )
    relationship_index = {item["id"]: item for item in iter_relationships(report)}
    supporting_relationships = _limit_deduped(
        rank_relationships(
            [
                relationship_index[relationship_id]
                for relationship_id in cluster.get("source_relationship_ids", [])
                if relationship_id in relationship_index
            ],
            anchor_paths=cluster["member_paths"],
        ),
        limit=5,
    )
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
        "member_hotspots": [record_summary(record) for record in member_records[:5]],
        "member_count": cluster["member_count"],
        "top_level_roots": cluster.get("top_level_roots", []),
    }
    payload["overlay_summary"] = {
        "organization_health": cluster,
        "member_overlay_maxima": descendant_overlay_maxima(member_records),
    }
    payload["supporting_relationships"] = supporting_relationships
    payload["supporting_clusters"] = [cluster]
    payload["evidence_summary"] = _evidence_summary(payload, mode="Cluster")
    return payload


def _build_relationship_payload(report: dict[str, Any], relationship_id: str) -> dict[str, Any]:
    relationship = relationship_by_id(report, relationship_id)
    if relationship is None:
        raise ValueError(f"No relationship found for '{relationship_id}'.")
    source_record = resolve_path(report, relationship["source_path"])
    target_record = resolve_path(report, relationship["target_path"])
    shared_clusters = _limit_deduped(
        shared_clusters_for_relationship(report, relationship),
        limit=5,
    )
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
        "source": record_summary(source_record),
        "target": record_summary(target_record),
    }
    payload["overlay_summary"] = {
        "organization_health": relationship,
        "source_overlays": source_record.get("overlays", {}) if source_record else {},
        "target_overlays": target_record.get("overlays", {}) if target_record else {},
    }
    payload["supporting_relationships"] = [relationship]
    payload["supporting_clusters"] = shared_clusters
    payload["evidence_summary"] = _evidence_summary(payload, mode="Relationship")
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
        "evidence_summary": {
            "detector_cost": [
                "top explanations preserve the current action_queue order"
            ],
            "strongest_overlays": [],
            "supporting_evidence": {
                "relationship_ids": [],
                "cluster_ids": [],
            },
            "interpretation": (
                "Top explanations describe detector ordering; they do not rerank "
                "hotspots or prove a refactor is required."
            ),
        },
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
    overlays = overlays or {}
    if not overlays:
        return ["- none"]
    organization_health = overlays.get("organization_health") or {}
    verification = overlays.get("verification") or {}
    navigation = overlays.get("navigation") or {}
    blast_radius = overlays.get("blast_radius") or {}
    stewardship = overlays.get("stewardship") or {}
    semantic_drift = overlays.get("semantic_drift") or {}
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


def _format_evidence_summary(summary: dict[str, Any]) -> list[str]:
    if not summary:
        return ["- none"]
    detector_cost = summary.get("detector_cost", []) or ["none"]
    strongest_overlays = summary.get("strongest_overlays", []) or ["none"]
    supporting = summary.get("supporting_evidence", {}) or {}
    relationship_ids = supporting.get("relationship_ids", []) or ["none"]
    cluster_ids = supporting.get("cluster_ids", []) or ["none"]
    return [
        f"- strongest detector costs: {'; '.join(detector_cost)}",
        f"- strongest overlays: {'; '.join(strongest_overlays)}",
        f"- supporting relationships: {', '.join(relationship_ids)}",
        f"- supporting clusters: {', '.join(cluster_ids)}",
        f"- interpretation: {summary.get('interpretation', 'evidence only')}",
    ]


def _append_evidence_summary(lines: list[str], payload: dict[str, Any]) -> None:
    lines.extend(["", "Evidence Summary"])
    lines.extend(_format_evidence_summary(payload.get("evidence_summary", {})))


def _render_file_entry(payload: dict[str, Any]) -> str:
    target = payload["target"]
    cost_summary = payload.get("cost_summary", {})
    lines = [
        f"Explain: path {target['path']} [{target['record_type']}]",
        "",
        "Hotspot Cost",
        (
            "- slop: "
            f"{target.get('slop_band')} ({target.get('slop_score')}) "
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
    _append_evidence_summary(lines, payload)
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)


def _render_folder_entry(payload: dict[str, Any]) -> str:
    target = payload["target"]
    cost_summary = payload.get("cost_summary", {})
    overlay_summary = payload.get("overlay_summary", {})
    descendant_hotspots = cost_summary.get("descendant_hotspots", [])[:5]
    descendant_maxima = overlay_summary.get("descendant_overlay_maxima", {})
    lines = [
        f"Explain: path {target['path']} [{target['record_type']}]",
        "",
        "Hotspot Cost",
        (
            "- slop: "
            f"{target.get('slop_band')} ({target.get('slop_score')}) "
            f"context={target.get('context_band')} "
            f"reasons={_format_reason_codes(target.get('reason_codes', []))}"
        ),
        (
            "- load: "
            f"max_file_tokens={cost_summary.get('load', {}).get('file_token_count', 0)}, "
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
        "- descendant hotspots:",
    ]
    lines.extend(
        [
            (
                f"  - {item['path']} slop={item.get('slop_band')} "
                f"slop_score={item.get('slop_score')} context={item.get('context_band')}"
            )
            for item in descendant_hotspots
        ]
        or ["  - none"]
    )
    lines.extend(["", "Overlay Evidence", *_format_overlay_lines(overlay_summary)])
    if descendant_maxima:
        organization_maxima = descendant_maxima.get("organization_health", {})
        verification_maxima = descendant_maxima.get("verification", {})
        navigation_maxima = descendant_maxima.get("navigation", {})
        blast_radius_maxima = descendant_maxima.get("blast_radius", {})
        semantic_drift_maxima = descendant_maxima.get("semantic_drift", {})
        lines.append(
            "- descendant overlay maxima: "
            f"organization.diffusion={organization_maxima.get('diffusion_pressure', 0.0):.3f}, "
            f"verification={verification_maxima.get('verification_gap', 0.0):.3f}, "
            f"navigation={navigation_maxima.get('navigation_pressure', 0.0):.3f}, "
            f"blast_radius={blast_radius_maxima.get('blast_radius_pressure', 0.0):.3f}, "
            "semantic_drift="
            f"{semantic_drift_maxima.get('semantic_drift_pressure', 0.0):.3f}"
        )
    lines.extend(["", "Supporting Relationships"])
    relationships = payload.get("supporting_relationships", [])[:3]
    lines.extend(
        [_format_relationship_brief(relationship) for relationship in relationships] or ["- none"]
    )
    lines.extend(["", "Supporting Clusters"])
    clusters = payload.get("supporting_clusters", [])[:3]
    lines.extend([_format_cluster_brief(cluster) for cluster in clusters] or ["- none"])
    _append_evidence_summary(lines, payload)
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
            f"slop={cost_summary.get('source', {}).get('slop_band')} "
            f"slop_score={cost_summary.get('source', {}).get('slop_score')}"
        ),
        (
            f"- target={target['target_path']} "
            f"slop={cost_summary.get('target', {}).get('slop_band')} "
            f"slop_score={cost_summary.get('target', {}).get('slop_score')}"
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
    _append_evidence_summary(lines, payload)
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
                f"  - {item['path']} slop={item.get('slop_band')} "
                f"slop_score={item.get('slop_score')} context={item.get('context_band')}"
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
        lines.append(
            "- member overlay maxima: "
            f"organization.diffusion="
            f"{maxima.get('organization_health', {}).get('diffusion_pressure', 0.0):.3f}, "
            f"verification={maxima.get('verification', {}).get('verification_gap', 0.0):.3f}, "
            f"navigation={maxima.get('navigation', {}).get('navigation_pressure', 0.0):.3f}, "
            f"blast_radius={maxima.get('blast_radius', {}).get('blast_radius_pressure', 0.0):.3f}, "
            "semantic_drift="
            f"{maxima.get('semantic_drift', {}).get('semantic_drift_pressure', 0.0):.3f}"
        )
    lines.extend(["", "Supporting Relationships"])
    relationships = payload.get("supporting_relationships", [])
    lines.extend(
        [_format_relationship_brief(relationship) for relationship in relationships] or ["- none"]
    )
    lines.extend(["", "Supporting Clusters"])
    clusters = payload.get("supporting_clusters", [])
    lines.extend([_format_cluster_brief(cluster) for cluster in clusters] or ["- none"])
    _append_evidence_summary(lines, payload)
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)


def _render_top_item(payload: dict[str, Any], *, ordinal: int) -> str:
    target = payload["target"]
    cost_summary = payload.get("cost_summary", {})
    strongest = strongest_pressures(payload.get("overlay_summary", {}), limit=3)
    overlay_summary = ", ".join(f"{label}={value:.3f}" for label, value in strongest)
    relationships = payload.get("supporting_relationships", [])[:2]
    clusters = payload.get("supporting_clusters", [])[:2]
    relationship_ids = ", ".join(item["id"] for item in relationships) or "none"
    cluster_ids = ", ".join(item["id"] for item in clusters) or "none"
    return "\n".join(
        [
            (
                f"{ordinal}. {target['path']} [{target.get('slop_band')}] "
                f"slop_score={target.get('slop_score')} context={target.get('context_band')}"
            ),
            (
                "   cost: "
                f"load={cost_summary.get('load', {}).get('load_pressure', 0.0):.3f} "
                f"volatility="
                f"{cost_summary.get('volatility', {}).get('volatility_pressure', 0.0):.3f} "
                f"coordination="
                f"{cost_summary.get('coordination', {}).get('coordination_pressure', 0.0):.3f}"
            ),
            f"   overlays: {overlay_summary}",
            f"   relationships: {relationship_ids}",
            f"   clusters: {cluster_ids}",
        ]
    )


def render_explain_text(payload: dict[str, Any]) -> str:
    selector_kind = payload.get("selector", {}).get("kind")
    if selector_kind == "top":
        count = payload.get("target", {}).get("count", 0)
        blocks = [f"Explain: top {count} hotspots"]
        for index, item in enumerate(payload.get("items", []), start=1):
            blocks.extend(["", _render_top_item(item, ordinal=index)])
        blocks.extend(["", "Evidence Summary"])
        blocks.extend(_format_evidence_summary(payload.get("evidence_summary", {})))
        blocks.extend(["", payload["boundary_note"]])
        return "\n".join(blocks)
    if selector_kind == "cluster":
        return _render_cluster_entry(payload)
    if selector_kind == "relationship":
        return _render_relationship_entry(payload)
    if payload.get("target", {}).get("record_type") == "folder":
        return _render_folder_entry(payload)
    return _render_file_entry(payload)
