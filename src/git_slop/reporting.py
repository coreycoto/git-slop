from __future__ import annotations

import json
import shutil
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any

import yaml

from .config import latest_dir, runs_dir
from .costs.organization import top_organization_file_overlays
from .graphs.clusters import clusters_for_path, folder_clusters_for_prefix
from .graphs.relationships import folder_relationships_for_prefix, relationships_for_path
from .scoring import CONTEXT_BAND_ORDER, SLOP_BAND_ORDER, build_folder_record
from .tokenization import context_band_for_tokens, context_pressure_for_tokens


def _folder_paths_for_file(path: str) -> list[str]:
    pure_path = PurePosixPath(path)
    parents = ["."]
    current = pure_path.parent
    while str(current) not in ("", "."):
        parents.append(current.as_posix())
        current = current.parent
    return parents


def _mean(values: list[float]) -> float:
    return sum(values) / max(1, len(values))


def _top_level_root(path: str) -> str:
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


def _is_pure_context_hotspot(reason_codes: list[str]) -> bool:
    token_cost_reasons = {"high_token_cost", "critical_token_cost"}
    return bool(reason_codes) and set(reason_codes).issubset(token_cost_reasons)


def _signal_label(item: dict[str, Any]) -> str:
    return "context-only" if item["is_pure_context_hotspot"] else "mixed"


def _overlay_folder_aggregate(
    *,
    overlay_name: str,
    folder_path: str,
    descendants: list[dict[str, Any]],
) -> dict[str, Any]:
    if not descendants:
        return {"path": folder_path}
    numeric_values: dict[str, list[float]] = defaultdict(list)
    boolean_counts: dict[str, int] = defaultdict(int)
    list_values: dict[str, list[str]] = defaultdict(list)
    for descendant in descendants:
        for key, value in descendant.items():
            if key == "path":
                continue
            if isinstance(value, bool):
                boolean_counts[key] += int(value)
            elif isinstance(value, int | float):
                numeric_values[key].append(float(value))
            elif isinstance(value, list):
                list_values[key].extend(str(item) for item in value)
    payload: dict[str, Any] = {"path": folder_path, "overlay_name": overlay_name}
    for key, values in numeric_values.items():
        if key.endswith("_count") or key.endswith("_degree") or key in {
            "path_depth",
            "sibling_count",
            "folder_width",
            "duplicate_name_count",
            "days_since_non_bot_edit",
        }:
            payload[key] = round(_mean(values), 6)
        else:
            payload[key] = round(_mean(values), 6)
    for key, count in boolean_counts.items():
        payload[key] = bool(count)
    for key, values in list_values.items():
        payload[key] = sorted(dict.fromkeys(values))[:20]
    payload["descendant_file_count"] = len(descendants)
    return payload


def _build_folder_costs(
    *,
    descendants: list[dict[str, Any]],
    config: dict[str, Any],
) -> dict[str, Any]:
    total_tokens = sum(int(record["tokens"]) for record in descendants)
    top_tokens = sorted((int(record["tokens"]) for record in descendants), reverse=True)
    revision_counts = [
        float(record["costs"]["volatility"]["commit_count_window"])
        for record in descendants
    ]
    relative_token_churn = [
        float(record["costs"]["volatility"]["relative_token_churn"]) for record in descendants
    ]
    change_diffusion = [
        float(record["costs"]["coordination"]["change_diffusion"]) for record in descendants
    ]
    coordination_pressure = [
        float(record["costs"]["coordination"]["coordination_pressure"]) for record in descendants
    ]
    return {
        "load": {
            "file_token_count": max(int(record["tokens"]) for record in descendants),
            "folder_token_count": total_tokens,
            "top_file_share": round(top_tokens[0] / max(1, total_tokens), 6),
            "top_3_file_share": round(sum(top_tokens[:3]) / max(1, total_tokens), 6),
            "token_concentration_ratio": round(top_tokens[0] / max(1, total_tokens), 6),
            "context_band": context_band_for_tokens(total_tokens, config),
            "load_pressure": round(context_pressure_for_tokens(total_tokens, config), 6),
        },
        "volatility": {
            "commit_count_window": round(sum(revision_counts), 6),
            "recency_weighted_commits": round(
                sum(
                    float(record["costs"]["volatility"]["recency_weighted_commits"])
                    for record in descendants
                ),
                6,
            ),
            "line_churn_window": round(
                sum(
                    float(record["costs"]["volatility"]["line_churn_window"])
                    for record in descendants
                ),
                6,
            ),
            "token_churn_window": round(
                sum(
                    float(record["costs"]["volatility"]["token_churn_window"])
                    for record in descendants
                ),
                6,
            ),
            "relative_token_churn": round(_mean(relative_token_churn), 6),
            "late_churn_spike": round(
                _mean(
                    [
                        float(record["costs"]["volatility"]["late_churn_spike"])
                        for record in descendants
                    ]
                ),
                6,
            ),
            "volatility_pressure": round(
                _mean(
                    [
                        float(record["costs"]["volatility"]["volatility_pressure"])
                        for record in descendants
                    ]
                ),
                6,
            ),
        },
        "coordination": {
            "files_touched_per_change": round(
                _mean(
                    [
                        float(record["costs"]["coordination"]["files_touched_per_change"])
                        for record in descendants
                    ]
                ),
                6,
            ),
            "folders_touched_per_change": round(
                _mean(
                    [
                        float(record["costs"]["coordination"]["folders_touched_per_change"])
                        for record in descendants
                    ]
                ),
                6,
            ),
            "edit_hunks_per_change": round(
                _mean(
                    [
                        float(record["costs"]["coordination"]["edit_hunks_per_change"])
                        for record in descendants
                    ]
                ),
                6,
            ),
            "cochange_degree": round(
                _mean(
                    [
                        float(record["costs"]["coordination"]["cochange_degree"])
                        for record in descendants
                    ]
                ),
                6,
            ),
            "cochange_centrality": round(
                _mean(
                    [
                        float(record["costs"]["coordination"]["cochange_centrality"])
                        for record in descendants
                    ]
                ),
                6,
            ),
            "cross_folder_cochange_ratio": round(
                _mean(
                    [
                        float(record["costs"]["coordination"]["cross_folder_cochange_ratio"])
                        for record in descendants
                    ]
                ),
                6,
            ),
            "change_diffusion": round(_mean(change_diffusion), 6),
            "coordination_pressure": round(_mean(coordination_pressure), 6),
        },
    }


def _build_folder_overlays(
    *,
    folder_path: str,
    descendants: list[dict[str, Any]],
) -> dict[str, Any]:
    overlay_names = [
        "organization_health",
        "verification",
        "navigation",
        "blast_radius",
        "stewardship",
        "semantic_drift",
    ]
    overlays: dict[str, Any] = {}
    for overlay_name in overlay_names:
        overlay_descendants = [
            (record.get("overlays") or {})[overlay_name]
            for record in descendants
            if (record.get("overlays") or {}).get(overlay_name) is not None
        ]
        if overlay_descendants:
            overlays[overlay_name] = _overlay_folder_aggregate(
                overlay_name=overlay_name,
                folder_path=folder_path,
                descendants=overlay_descendants,
            )
        else:
            overlays[overlay_name] = None
    return overlays


def build_folder_records(
    file_records: list[dict[str, Any]], config: dict[str, Any]
) -> list[dict[str, Any]]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for record in file_records:
        for folder_path in _folder_paths_for_file(record["path"]):
            grouped[folder_path].append(record)
    records: list[dict[str, Any]] = []
    for folder_path, descendants in grouped.items():
        base_record = build_folder_record(path=folder_path, descendants=descendants, config=config)
        base_record["costs"] = _build_folder_costs(descendants=descendants, config=config)
        base_record["overlays"] = _build_folder_overlays(
            folder_path=folder_path,
            descendants=descendants,
        )
        records.append(base_record)
    return sorted(records, key=lambda record: (record["path"] != ".", record["path"]))


def build_action_queue(
    file_records: list[dict[str, Any]], *, limit: int = 25
) -> list[dict[str, Any]]:
    sorted_records = sorted(
        file_records,
        key=lambda record: (-record["slop_score"], -record["tokens"], record["path"]),
    )
    queue = []
    for record in sorted_records[:limit]:
        queue.append(
            {
                "path": record["path"],
                "slop_score": record["slop_score"],
                "slop_band": record["slop_band"],
                "context_band": record["context_band"],
                "tokens": record["tokens"],
                "age_days": record["age_days"],
                "revisions_window": record["revisions_window"],
                "churn_pressure": record["churn_pressure"],
                "reason_codes": record["reason_codes"],
                "is_pure_context_hotspot": _is_pure_context_hotspot(record["reason_codes"]),
            }
        )
    return queue


def _file_map(records: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {record["path"]: record for record in records}


def _canonical_organization_overlay(organization_analysis: dict[str, Any]) -> dict[str, Any]:
    return {
        "enabled": True,
        "experimental": True,
        "analysis_status": organization_analysis["organization_metrics"]["analysis_status"],
        "analysis_version": organization_analysis["organization_metrics"]["analysis_version"],
        "repo_baselines": organization_analysis["organization_metrics"]["repo_baselines"],
        "files": organization_analysis["organization_metrics"]["files"],
        "folders": organization_analysis["organization_metrics"]["folders"],
        "relationships": organization_analysis["relationships"],
        "clusters": organization_analysis["clusters"],
        "findings": {
            "top_structural_files": top_organization_file_overlays(
                {
                    "organization_metrics": organization_analysis["organization_metrics"],
                    "relationships": organization_analysis["relationships"],
                    "clusters": organization_analysis["clusters"],
                },
                limit=10,
            ),
            "top_consolidation_candidates": organization_analysis["clusters"][
                "consolidation_candidates"
            ][:10],
        },
    }


def _overlay_with_folders(
    overlay: dict[str, Any],
    file_records: list[dict[str, Any]],
    name: str,
) -> dict[str, Any]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for record in overlay.get("files", []):
        for folder_path in _folder_paths_for_file(record["path"]):
            grouped[folder_path].append(record)
    folders = [
        _overlay_folder_aggregate(
            overlay_name=name,
            folder_path=folder_path,
            descendants=descendants,
        )
        for folder_path, descendants in grouped.items()
    ]
    return {
        **overlay,
        "enabled": True,
        "experimental": True,
        "folders": sorted(folders, key=lambda item: (item["path"] != ".", item["path"])),
    }


def _summary_block(
    *,
    action_queue: list[dict[str, Any]],
    overlays: dict[str, Any],
) -> dict[str, Any]:
    return {
        "top_hotspots": [item["path"] for item in action_queue[:5]],
        "top_structural_files": [
            item["path"]
            for item in overlays["organization_health"]["findings"]["top_structural_files"][:5]
        ],
        "top_verification_gaps": [
            item["path"]
            for item in sorted(
                overlays["verification"]["files"],
                key=lambda item: (-item["verification_gap"], item["path"]),
            )[:5]
        ],
    }


def build_report(
    *,
    repo: dict[str, Any],
    config: dict[str, Any],
    file_records: list[dict[str, Any]],
    folder_records: list[dict[str, Any]],
    action_queue: list[dict[str, Any]],
    stable_costs: dict[str, dict[str, dict[str, Any]]],
    overlay_results: dict[str, Any],
    skipped: dict[str, int],
    generated_at: str,
) -> dict[str, Any]:
    critical_context_count = sum(
        1 for record in file_records if record["context_band"] == "critical"
    )
    critical_slop_count = sum(
        1 for record in file_records if record["slop_band"] == "critical"
    )

    canonical_overlays = {
        "organization_health": _canonical_organization_overlay(
            overlay_results["organization_health"]
        ),
        "verification": _overlay_with_folders(
            overlay_results["verification"],
            file_records,
            "verification",
        ),
        "navigation": _overlay_with_folders(
            overlay_results["navigation"],
            file_records,
            "navigation",
        ),
        "blast_radius": _overlay_with_folders(
            overlay_results["blast_radius"],
            file_records,
            "blast_radius",
        ),
        "stewardship": _overlay_with_folders(
            overlay_results["stewardship"],
            file_records,
            "stewardship",
        ),
        "semantic_drift": _overlay_with_folders(
            overlay_results["semantic_drift"],
            file_records,
            "semantic_drift",
        ),
    }
    org_file_overlays = _file_map(canonical_overlays["organization_health"]["files"])
    verification_by_path = _file_map(canonical_overlays["verification"]["files"])
    navigation_by_path = _file_map(canonical_overlays["navigation"]["files"])
    blast_radius_by_path = _file_map(canonical_overlays["blast_radius"]["files"])
    stewardship_by_path = _file_map(canonical_overlays["stewardship"]["files"])
    semantic_drift_by_path = _file_map(canonical_overlays["semantic_drift"]["files"])

    files_payload: list[dict[str, Any]] = []
    for record in sorted(file_records, key=lambda item: item["path"]):
        payload = {key: value for key, value in record.items() if key != "text"}
        payload["costs"] = {
            "load": stable_costs["load"][record["path"]],
            "volatility": stable_costs["volatility"][record["path"]],
            "coordination": stable_costs["coordination"][record["path"]],
        }
        payload["overlays"] = {
            "organization_health": org_file_overlays.get(record["path"]),
            "verification": verification_by_path.get(record["path"]),
            "navigation": navigation_by_path.get(record["path"]),
            "blast_radius": blast_radius_by_path.get(record["path"]),
            "stewardship": stewardship_by_path.get(record["path"]),
            "semantic_drift": semantic_drift_by_path.get(record["path"]),
        }
        files_payload.append(payload)

    folder_records = build_folder_records(files_payload, config)

    costs_summary = {
        "load": {"analysis_status": "stable", "analysis_version": 1},
        "volatility": {"analysis_status": "stable", "analysis_version": 1},
        "coordination": {"analysis_status": "stable", "analysis_version": 1},
    }

    report = {
        "schema_version": 4,
        "generated_at": generated_at,
        "summary": _summary_block(action_queue=action_queue, overlays=canonical_overlays),
        "repo": repo,
        "config": config,
        "stats": {
            "tracked_file_count": len(file_records)
            + skipped["ignored"]
            + skipped["missing"]
            + skipped["binary"]
            + skipped["undecodable"],
            "analyzed_file_count": len(file_records),
            "skipped_ignored_count": skipped["ignored"],
            "skipped_missing_count": skipped["missing"],
            "skipped_binary_count": skipped["binary"],
            "skipped_undecodable_count": skipped["undecodable"],
            "critical_context_file_count": critical_context_count,
            "critical_slop_file_count": critical_slop_count,
        },
        "files": files_payload,
        "folders": folder_records,
        "action_queue": action_queue,
        "costs": costs_summary,
        "overlays": canonical_overlays,
        "organization_metrics": overlay_results["organization_health"]["organization_metrics"],
        "relationships": overlay_results["organization_health"]["relationships"],
        "clusters": overlay_results["organization_health"]["clusters"],
    }
    return report


def render_terminal_table(action_queue: list[dict[str, Any]]) -> str:
    if not action_queue:
        return "No hotspot records found."
    path_width = max(len("Path"), min(64, max(len(item["path"]) for item in action_queue)))
    header = (
        f"{'Path':<{path_width}}  {'Slop':<8}  {'Context':<8}  "
        f"{'SlopScore':>9}  {'Tokens':>8}  {'Age':>5}  {'Revs':>5}  {'Churn':>6}  {'Signal':<12}"
    )
    lines = [
        header,
        (
            f"{'-' * path_width}  {'-' * 8}  {'-' * 8}  {'-' * 9}  "
            f"{'-' * 8}  {'-' * 5}  {'-' * 5}  {'-' * 6}  {'-' * 12}"
        ),
    ]
    for item in action_queue:
        path = (
            item["path"]
            if len(item["path"]) <= path_width
            else f"...{item['path'][-(path_width - 3) :]}"
        )
        lines.append(
            f"{path:<{path_width}}  "
            f"{item['slop_band']:<8}  "
            f"{item['context_band']:<8}  "
            f"{item['slop_score']:>9.1f}  "
            f"{item['tokens']:>8}  "
            f"{item['age_days']:>5}  "
            f"{item['revisions_window']:>5}  "
            f"{item['churn_pressure']:>6.3f}  "
            f"{_signal_label(item):<12}"
        )
    return "\n".join(lines)


def _render_organization_terminal(report: dict[str, Any]) -> str:
    organization_overlay = report["overlays"]["organization_health"]
    overlays = organization_overlay["findings"]["top_structural_files"][:5]
    duplicate_candidates = sorted(
        organization_overlay["relationships"]["duplicate_neighborhoods"]
        + organization_overlay["relationships"]["near_duplicate_neighborhoods"],
        key=lambda item: (-item["evidence_score"], item["id"]),
    )[:5]
    coupling_edges = organization_overlay["relationships"]["temporal_coupling_edges"][:5]
    consolidation_candidates = organization_overlay["clusters"]["consolidation_candidates"][:5]

    lines = ["Organization Health", ""]
    if overlays:
        path_width = max(len("Path"), min(64, max(len(item["path"]) for item in overlays)))
        header = (
            f"{'Path':<{path_width}}  {'Dup':>5}  {'Diff':>5}  {'Coup':>5}  {'Bound':>5}  "
            f"{'DupRatio':>8}  {'HiDiff':>6}  {'Cross':>5}"
        )
        lines.extend(
            [
                header,
                (
                    f"{'-' * path_width}  {'-' * 5}  {'-' * 5}  {'-' * 5}  {'-' * 5}  "
                    f"{'-' * 8}  {'-' * 6}  {'-' * 5}"
                ),
            ]
        )
        for item in overlays:
            path = (
                item["path"]
                if len(item["path"]) <= path_width
                else f"...{item['path'][-(path_width - 3) :]}"
            )
            lines.append(
                f"{path:<{path_width}}  "
                f"{item['duplication_pressure']:>5.3f}  "
                f"{item['diffusion_pressure']:>5.3f}  "
                f"{item['coupling_pressure']:>5.3f}  "
                f"{item['boundary_pressure']:>5.3f}  "
                f"{item['duplicate_token_ratio']:>8.3f}  "
                f"{item['high_diffusion_commit_count']:>6}  "
                f"{item['cross_boundary_edge_count']:>5}"
            )
    else:
        lines.append("No organization-health file overlays found.")

    lines.extend(["", "Top Duplicate / Near-Duplicate Pairs"])
    if duplicate_candidates:
        for item in duplicate_candidates:
            lines.append(
                f"- {item['source_path']} <-> {item['target_path']} "
                f"({item['kind']}, score {item['evidence_score']:.3f})"
            )
    else:
        lines.append("- None")

    lines.extend(["", "Top Temporal Coupling Edges"])
    if coupling_edges:
        for item in coupling_edges:
            lines.append(
                f"- {item['source_path']} <-> {item['target_path']} "
                f"(support {item['support_count']}, lift {item['lift_score']:.3f})"
            )
    else:
        lines.append("- None")

    lines.extend(["", "Top Consolidation Candidates"])
    if consolidation_candidates:
        for item in consolidation_candidates:
            members = ", ".join(item["member_paths"][:4])
            if item["member_count"] > 4:
                members += ", ..."
            lines.append(
                f"- {item['candidate_type']} [{item['member_count']} files, "
                f"score {item['evidence_score']:.3f}]: {members}"
            )
    else:
        lines.append("- None")

    return "\n".join(lines)


def _render_overlay_highlights(report: dict[str, Any]) -> str:
    verification = sorted(
        report["overlays"]["verification"]["files"],
        key=lambda item: (-item["verification_gap"], item["path"]),
    )[:5]
    navigation = sorted(
        report["overlays"]["navigation"]["files"],
        key=lambda item: (-item["navigation_pressure"], item["path"]),
    )[:5]
    blast_radius = sorted(
        report["overlays"]["blast_radius"]["files"],
        key=lambda item: (-item["blast_radius_pressure"], item["path"]),
    )[:5]
    lines = ["Other Overlay Highlights", ""]
    lines.append("Top Verification Gaps")
    if verification:
        lines.extend(
            f"- {item['path']} (gap {item['verification_gap']:.3f})" for item in verification
        )
    else:
        lines.append("- None")
    lines.extend(["", "Top Navigation Pressure"])
    if navigation:
        lines.extend(
            f"- {item['path']} (pressure {item['navigation_pressure']:.3f})"
            for item in navigation
        )
    else:
        lines.append("- None")
    lines.extend(["", "Top Blast Radius"])
    if blast_radius:
        lines.extend(
            f"- {item['path']} (pressure {item['blast_radius_pressure']:.3f})"
            for item in blast_radius
        )
    else:
        lines.append("- None")
    return "\n".join(lines)


def render_terminal_output(report: dict[str, Any]) -> str:
    return "\n\n".join(
        [
            render_terminal_table(report["action_queue"]),
            _render_organization_terminal(report),
            _render_overlay_highlights(report),
        ]
    )


def render_summary(report: dict[str, Any]) -> str:
    lines = [
        "# Git Slop Summary",
        "",
        f"- Repository: `{report['repo']['repo_name']}`",
        f"- Snapshot timestamp: `{report['generated_at']}`",
        f"- Branch: `{report['repo']['branch'] or 'detached'}`",
        f"- Head commit: `{report['repo']['head_commit'] or 'none'}`",
        f"- Analyzed files: {report['stats']['analyzed_file_count']}",
        f"- Skipped ignored: {report['stats']['skipped_ignored_count']}",
        f"- Skipped missing: {report['stats']['skipped_missing_count']}",
        f"- Skipped binary: {report['stats']['skipped_binary_count']}",
        f"- Skipped undecodable: {report['stats']['skipped_undecodable_count']}",
        f"- Critical context files: {report['stats']['critical_context_file_count']}",
        f"- Critical slop files: {report['stats']['critical_slop_file_count']}",
        "",
        "## Top Hotspots",
        "",
        "| Path | Slop | Context | Slop Score | Tokens | Age | Revs | Churn | Signal | Reasons |",
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |",
    ]
    for item in report["action_queue"]:
        lines.append(
            f"| `{item['path']}` | `{item['slop_band']}` | `{item['context_band']}` | "
            f"{item['slop_score']:.1f} | {item['tokens']} | {item['age_days']} | "
            f"{item['revisions_window']} | {item['churn_pressure']:.3f} | "
            f"`{_signal_label(item)}` | {', '.join(item['reason_codes']) or '_none_'} |"
        )

    lines.extend(["", "## Organization Health", ""])
    overlays = report["overlays"]["organization_health"]["findings"]["top_structural_files"][:5]
    lines.extend(
        [
            "### Top Structural Files",
            "",
            (
                "| Path | Dup | Diff | Coup | Bound | Dup Ratio | High Diff | "
                "Cross Boundary |"
            ),
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
        ]
    )
    for item in overlays:
        lines.append(
            f"| `{item['path']}` | {item['duplication_pressure']:.3f} | "
            f"{item['diffusion_pressure']:.3f} | {item['coupling_pressure']:.3f} | "
            f"{item['boundary_pressure']:.3f} | {item['duplicate_token_ratio']:.3f} | "
            f"{item['high_diffusion_commit_count']} | {item['cross_boundary_edge_count']} |"
        )
    if not overlays:
        lines.append("| _none_ | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0 | 0 |")

    duplicate_candidates = sorted(
        report["overlays"]["organization_health"]["relationships"]["duplicate_neighborhoods"]
        + report["overlays"]["organization_health"]["relationships"][
            "near_duplicate_neighborhoods"
        ],
        key=lambda item: (-item["evidence_score"], item["id"]),
    )[:5]
    lines.extend(
        [
            "",
            "### Top Duplicate / Near-Duplicate Pairs",
            "",
            "| Pair | Kind | Score | Boundary |",
            "| --- | --- | ---: | --- |",
        ]
    )
    for item in duplicate_candidates:
        lines.append(
            f"| `{item['source_path']}` ↔ `{item['target_path']}` | "
            f"`{item['kind']}` | {item['evidence_score']:.3f} | "
            f"`{'cross-root' if item['crosses_top_level_boundary'] else 'local'}` |"
        )
    if not duplicate_candidates:
        lines.append("| _none_ | _none_ | 0.000 | `_none_` |")

    lines.extend(["", "## Overlay Highlights", ""])
    verification = sorted(
        report["overlays"]["verification"]["files"],
        key=lambda item: (-item["verification_gap"], item["path"]),
    )[:5]
    lines.extend(
        [
            "### Verification Gaps",
            "",
            "| Path | Gap | Nearby Tests | Test Cochange |",
            "| --- | ---: | --- | ---: |",
        ]
    )
    for item in verification:
        preview = ", ".join(f"`{path}`" for path in item["nearby_test_paths"][:3]) or "_none_"
        lines.append(
            f"| `{item['path']}` | {item['verification_gap']:.3f} | {preview} | "
            f"{item['test_cochange_ratio']:.3f} |"
        )
    if not verification:
        lines.append("| _none_ | 0.000 | _none_ | 0.000 |")

    lines.extend(["", "## Next Action Queue", ""])
    if report["action_queue"]:
        for index, item in enumerate(report["action_queue"], start=1):
            lines.append(
                f"{index}. `{item['path']}` "
                f"({item['slop_band']}, {item['context_band']}, "
                f"slop_score {item['slop_score']:.1f}, {item['tokens']} tokens, "
                f"{_signal_label(item)})"
            )
    else:
        lines.append("No hotspot records found.")
    return "\n".join(lines) + "\n"


def _bundle_payloads(report: dict[str, Any]) -> dict[str, str]:
    return {
        "report.json": json.dumps(report, indent=2, sort_keys=True) + "\n",
        "report.yaml": yaml.safe_dump(report, sort_keys=False),
        "summary.md": render_summary(report),
    }


def _write_bundle_files(output_root: Path, bundle_payloads: dict[str, str]) -> None:
    output_root.mkdir(parents=True, exist_ok=True)
    for file_name, content in bundle_payloads.items():
        (output_root / file_name).write_text(content, encoding="utf-8")


def _replace_latest_bundle(
    latest_root: Path,
    bundle_payloads: dict[str, str],
    *,
    run_slug: str,
) -> None:
    temp_root = latest_root.parent / f".latest-{run_slug}.tmp"
    backup_root = latest_root.parent / f".latest-{run_slug}.bak"
    shutil.rmtree(temp_root, ignore_errors=True)
    shutil.rmtree(backup_root, ignore_errors=True)

    try:
        _write_bundle_files(temp_root, bundle_payloads)
        if latest_root.exists():
            latest_root.rename(backup_root)
        try:
            temp_root.rename(latest_root)
        except Exception:
            if backup_root.exists() and not latest_root.exists():
                backup_root.rename(latest_root)
            raise
        if backup_root.exists():
            shutil.rmtree(backup_root)
    except Exception:
        shutil.rmtree(temp_root, ignore_errors=True)
        if backup_root.exists() and not latest_root.exists():
            backup_root.rename(latest_root)
        raise


def write_report_bundle(
    *,
    repo_root: Path,
    report: dict[str, Any],
    run_slug: str,
) -> dict[str, str]:
    run_root = runs_dir(repo_root) / run_slug
    latest_root = latest_dir(repo_root)
    latest_root.mkdir(parents=True, exist_ok=True)

    bundle_payloads = _bundle_payloads(report)
    _write_bundle_files(run_root, bundle_payloads)
    _replace_latest_bundle(latest_root, bundle_payloads, run_slug=run_slug)
    return {
        "run_root": str(run_root),
        "latest_root": str(latest_root),
        "report_json": str(latest_root / "report.json"),
        "report_yaml": str(latest_root / "report.yaml"),
        "summary_md": str(latest_root / "summary.md"),
    }


def load_report(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def find_report_record(report: dict[str, Any], target_path: str) -> dict[str, Any] | None:
    normalized = target_path.strip() or "."
    for collection_name in ("files", "folders"):
        for record in report[collection_name]:
            if record["path"] == normalized:
                return dict(record)
    return None


def build_show_payload(report: dict[str, Any], target_path: str) -> dict[str, Any] | None:
    normalized = target_path.strip() or "."
    base_record = find_report_record(report, normalized)
    if base_record is None:
        return None
    is_file = any(record["path"] == normalized for record in report["files"])
    if is_file:
        strongest_relationships = relationships_for_path(report, normalized)[:10]
        cluster_memberships = clusters_for_path(report, normalized)[:10]
    else:
        strongest_relationships = folder_relationships_for_prefix(report, normalized)[:10]
        cluster_memberships = folder_clusters_for_prefix(report, normalized)[:10]
    payload = dict(base_record)
    payload["record_type"] = "file" if is_file else "folder"
    payload["organization_health"] = (payload.get("overlays") or {}).get("organization_health")
    payload["strongest_relationships"] = strongest_relationships
    payload["cluster_memberships"] = cluster_memberships
    return payload


def failing_records(
    report: dict[str, Any],
    *,
    fail_on_context_band: str | None,
    fail_on_slop_band: str | None,
) -> list[dict[str, Any]]:
    failures: list[dict[str, Any]] = []
    for record in report["files"]:
        failed = False
        if fail_on_context_band is not None:
            failed = (
                CONTEXT_BAND_ORDER[record["context_band"]]
                >= CONTEXT_BAND_ORDER[fail_on_context_band]
            )
        if fail_on_slop_band is not None:
            failed = failed or (
                SLOP_BAND_ORDER[record["slop_band"]]
                >= SLOP_BAND_ORDER[fail_on_slop_band]
            )
        if failed:
            failures.append(record)
    return sorted(
        failures,
        key=lambda record: (-record["slop_score"], -record["tokens"], record["path"]),
    )


def utc_timestamp_slug() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
