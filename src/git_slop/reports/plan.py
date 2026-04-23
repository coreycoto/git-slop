from __future__ import annotations

from typing import Any

from git_slop.reports.shared import (
    EXPECTED_REPORT_SCHEMA_VERSION,
    all_clusters_for_record,
    all_relationships_for_record,
    cluster_by_id,
    dedupe_by_id,
    descendant_file_records,
    descendant_hotspots,
    iter_relationships,
    rank_clusters,
    rank_relationships,
    relationship_by_id,
    resolve_path,
    shared_clusters_for_relationship,
    unique_preserving_order,
)

PLAN_SCHEMA_VERSION = 1
MAX_SLICE_FILES = 5
BOUNDARY_NOTE = (
    "Plan boundary: this is a bounded proposal only. It does not mutate code, "
    "GitHub, or detector truth, and it does not guarantee correctness or safety."
)


def _slug(value: str) -> str:
    return (
        value.replace("/", "--")
        .replace(".", "_")
        .replace(":", "_")
        .replace(" ", "-")
    )


def _sort_paths_by_severity(
    report: dict[str, Any],
    paths: list[str],
    *,
    anchor_paths: list[str],
) -> list[str]:
    path_records = {
        path: resolve_path(report, path)
        for path in dict.fromkeys(paths)
    }
    anchor_order = {path: index for index, path in enumerate(anchor_paths)}
    sorted_paths = [
        path
        for path, _record in sorted(
            path_records.items(),
            key=lambda item: (
                0 if item[0] in anchor_order else 1,
                anchor_order.get(item[0], 10_000),
                -float((item[1] or {}).get("priority_score") or 0.0),
                item[0],
            ),
        )
        if path_records[path] is not None
    ]
    return sorted_paths


def _build_scope(
    report: dict[str, Any],
    *,
    candidate_paths: list[str],
    anchor_paths: list[str],
) -> tuple[list[str], list[str]]:
    ordered = _sort_paths_by_severity(report, candidate_paths, anchor_paths=anchor_paths)
    return ordered[:MAX_SLICE_FILES], ordered[MAX_SLICE_FILES:]


def _build_folder_anchor_paths(report: dict[str, Any], folder_path: str) -> list[str]:
    hotspots = [
        item["path"]
        for item in descendant_hotspots(
            report,
            folder_path,
            limit=MAX_SLICE_FILES,
        )
    ]
    if hotspots:
        return hotspots
    return [
        record["path"]
        for record in descendant_file_records(report, folder_path)[:MAX_SLICE_FILES]
    ]


def _folder_cluster_scope_paths(
    report: dict[str, Any],
    *,
    folder_path: str,
    cluster: dict[str, Any],
) -> list[str]:
    descendants = [
        path
        for path in cluster["member_paths"]
        if path.startswith(f"{folder_path.rstrip('/')}/")
    ]
    external = [path for path in cluster["member_paths"] if path not in descendants]
    ordered_external = _sort_paths_by_severity(report, external, anchor_paths=[])
    return descendants + ordered_external[:1]


def _slice_payload(
    *,
    slice_id: str,
    title: str,
    scope_paths: list[str],
    out_of_scope_paths: list[str],
    supporting_relationship_ids: list[str],
    supporting_cluster_ids: list[str],
    why_this_slice: str,
    ranking_reason: str,
) -> dict[str, Any]:
    return {
        "id": slice_id,
        "title": title,
        "scope_paths": scope_paths,
        "out_of_scope_paths": out_of_scope_paths,
        "supporting_relationship_ids": supporting_relationship_ids,
        "supporting_cluster_ids": supporting_cluster_ids,
        "why_this_slice": why_this_slice,
        "ranking_reason": ranking_reason,
    }


def _build_path_target(record: dict[str, Any]) -> dict[str, Any]:
    return {
        "kind": "path",
        "path": record["path"],
        "record_type": record["record_type"],
        "priority_score": record.get("priority_score"),
        "priority_band": record.get("priority_band"),
        "context_band": record.get("context_band"),
        "reason_codes": record.get("reason_codes", []),
    }


def _build_cluster_target(cluster: dict[str, Any]) -> dict[str, Any]:
    return {
        "kind": "cluster",
        "id": cluster["id"],
        "cluster_kind": cluster["kind"],
        "candidate_type": cluster.get("candidate_type"),
        "member_count": cluster["member_count"],
        "member_paths": cluster["member_paths"],
        "top_level_roots": cluster.get("top_level_roots", []),
    }


def _build_relationship_target(relationship: dict[str, Any]) -> dict[str, Any]:
    return {
        "kind": "relationship",
        "id": relationship["id"],
        "relationship_kind": relationship["kind"],
        "source_path": relationship["source_path"],
        "target_path": relationship["target_path"],
        "evidence_score": relationship["evidence_score"],
    }


def _anchor_context_for_path(report: dict[str, Any], path: str) -> dict[str, Any]:
    record = resolve_path(report, path)
    if record is None:
        raise ValueError(f"No record found for '{path}'.")
    if record["record_type"] == "folder":
        anchor_paths = _build_folder_anchor_paths(report, record["path"])
        focus_folder = record["path"]
    else:
        anchor_paths = [record["path"]]
        focus_folder = None
    supporting_relationships = rank_relationships(
        all_relationships_for_record(report, record),
        anchor_paths=anchor_paths,
        focus_folder=focus_folder,
    )
    supporting_clusters = rank_clusters(
        all_clusters_for_record(report, record),
        anchor_paths=anchor_paths,
        focus_folder=focus_folder,
    )
    return {
        "selector": {"kind": "path", "value": path},
        "target": _build_path_target(record),
        "anchor_paths": anchor_paths,
        "supporting_relationships": supporting_relationships,
        "supporting_clusters": supporting_clusters,
        "focus_folder": focus_folder,
        "record": record,
    }


def _anchor_context_for_cluster(report: dict[str, Any], cluster_id: str) -> dict[str, Any]:
    cluster = cluster_by_id(report, cluster_id)
    if cluster is None:
        raise ValueError(f"No cluster found for '{cluster_id}'.")
    relationship_index = {
        relationship["id"]: relationship
        for relationship in dedupe_by_id(iter_relationships(report))
    }
    supporting_relationships = rank_relationships(
        [
            relationship_index[relationship_id]
            for relationship_id in cluster.get("source_relationship_ids", [])
            if relationship_id in relationship_index
        ],
        anchor_paths=cluster["member_paths"],
    )
    return {
        "selector": {"kind": "cluster", "value": cluster_id},
        "target": _build_cluster_target(cluster),
        "anchor_paths": cluster["member_paths"],
        "supporting_relationships": supporting_relationships,
        "supporting_clusters": [cluster],
        "focus_folder": None,
        "cluster": cluster,
    }


def _anchor_context_for_relationship(
    report: dict[str, Any],
    relationship_id: str,
) -> dict[str, Any]:
    relationship = relationship_by_id(report, relationship_id)
    if relationship is None:
        raise ValueError(f"No relationship found for '{relationship_id}'.")
    shared_clusters = shared_clusters_for_relationship(report, relationship)
    return {
        "selector": {"kind": "relationship", "value": relationship_id},
        "target": _build_relationship_target(relationship),
        "anchor_paths": [relationship["source_path"], relationship["target_path"]],
        "supporting_relationships": [relationship],
        "supporting_clusters": shared_clusters,
        "focus_folder": None,
        "relationship": relationship,
    }


def _build_anchor_slice(report: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    target = context["target"]
    selector_kind = context["selector"]["kind"]
    candidate_paths = context["anchor_paths"]
    if (
        selector_kind == "cluster"
        and target["member_count"] > MAX_SLICE_FILES
        and context["supporting_relationships"]
    ):
        leading_relationship = context["supporting_relationships"][0]
        candidate_paths = [
            leading_relationship["source_path"],
            leading_relationship["target_path"],
        ]
    scope_paths, out_of_scope_paths = _build_scope(
        report,
        candidate_paths=candidate_paths,
        anchor_paths=context["anchor_paths"],
    )
    if selector_kind == "path" and target["record_type"] == "folder":
        title = f"Focus descendant hotspots in {target['path']}"
        why = (
            "Start with the highest-ranked descendant hotspots already driving "
            "this folder's context cost."
        )
    elif selector_kind == "path":
        title = f"Anchor hotspot {target['path']}"
        why = "Start with the selected hotspot before expanding to adjacent structural evidence."
    elif selector_kind == "cluster":
        if target["member_count"] > MAX_SLICE_FILES and context["supporting_relationships"]:
            title = f"Start inside cluster {target['id']}"
            why = (
                "The selected cluster is broad, so start with the strongest direct "
                "relationship-backed slice inside it before expanding."
            )
        else:
            title = f"Inspect cluster {target['id']}"
            why = (
                "Start with the selected cluster members before splitting work into "
                "narrower relationship-driven slices."
            )
    else:
        title = f"Inspect relationship {target['id']}"
        why = (
            "Start with the selected coupled pair before considering any "
            "surrounding cluster evidence."
        )
    return _slice_payload(
        slice_id=f"anchor-{selector_kind}-{_slug(context['selector']['value'])}",
        title=title,
        scope_paths=scope_paths,
        out_of_scope_paths=out_of_scope_paths,
        supporting_relationship_ids=[
            item["id"] for item in context["supporting_relationships"][:3]
        ],
        supporting_cluster_ids=[item["id"] for item in context["supporting_clusters"][:3]],
        why_this_slice=why,
        ranking_reason=(
            "Anchor slice always ranks first and keeps anchor paths ahead of "
            "any secondary evidence."
        ),
    )


def _build_relationship_slices(
    report: dict[str, Any],
    context: dict[str, Any],
) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, ...], dict[str, Any]] = {}
    focus_folder = context.get("focus_folder")
    anchor_paths = context["anchor_paths"]
    for relationship in context["supporting_relationships"]:
        candidate_paths = [relationship["source_path"], relationship["target_path"]]
        if focus_folder is not None:
            folder_prefix = f"{focus_folder.rstrip('/')}/"
            in_folder = [path for path in candidate_paths if path.startswith(folder_prefix)]
            out_of_folder = [path for path in candidate_paths if path not in in_folder]
            if not in_folder:
                continue
            candidate_paths = in_folder + out_of_folder[:1]
        scope_paths, out_of_scope_paths = _build_scope(
            report,
            candidate_paths=candidate_paths,
            anchor_paths=anchor_paths,
        )
        shared_clusters = shared_clusters_for_relationship(report, relationship)
        fingerprint = tuple(scope_paths)
        cluster_ids = [cluster["id"] for cluster in shared_clusters[:3]]
        existing = grouped.get(fingerprint)
        if existing is None:
            grouped[fingerprint] = _slice_payload(
                slice_id=f"relationship-{relationship['id']}",
                title=f"Inspect relationship {relationship['id']}",
                scope_paths=scope_paths,
                out_of_scope_paths=out_of_scope_paths,
                supporting_relationship_ids=[relationship["id"]],
                supporting_cluster_ids=cluster_ids,
                why_this_slice=(
                    "This pair already co-occurs in direct detector evidence "
                    "and should be reviewed together."
                ),
                ranking_reason=(
                    "Relationship slices rank after the anchor slice, ordered "
                    "by direct evidence strength."
                ),
            )
            continue
        existing["supporting_relationship_ids"] = sorted(
            set(existing["supporting_relationship_ids"] + [relationship["id"]])
        )
        existing["supporting_cluster_ids"] = sorted(
            set(existing["supporting_cluster_ids"] + cluster_ids)
        )[:3]
        existing["out_of_scope_paths"] = sorted(
            set(existing["out_of_scope_paths"] + out_of_scope_paths)
        )
    return list(grouped.values())


def _build_cluster_slices(report: dict[str, Any], context: dict[str, Any]) -> list[dict[str, Any]]:
    slices: list[dict[str, Any]] = []
    focus_folder = context.get("focus_folder")
    anchor_paths = context["anchor_paths"]
    for cluster in context["supporting_clusters"]:
        candidate_paths = list(cluster["member_paths"])
        if focus_folder is not None:
            candidate_paths = _folder_cluster_scope_paths(
                report,
                folder_path=focus_folder,
                cluster=cluster,
            )
        scope_paths, out_of_scope_paths = _build_scope(
            report,
            candidate_paths=candidate_paths,
            anchor_paths=anchor_paths,
        )
        omitted_paths = [path for path in cluster["member_paths"] if path not in scope_paths]
        combined_out_of_scope = dedupe_by_id(
            [{"id": path, "path": path} for path in out_of_scope_paths + omitted_paths]
        )
        slices.append(
            _slice_payload(
                slice_id=f"cluster-{cluster['id']}",
                title=f"Inspect cluster {cluster['id']}",
                scope_paths=scope_paths,
                out_of_scope_paths=[item["path"] for item in combined_out_of_scope],
                supporting_relationship_ids=cluster.get("source_relationship_ids", [])[:3],
                supporting_cluster_ids=[cluster["id"]],
                why_this_slice=(
                    "This slice stays inside one direct structural cluster "
                    "instead of sweeping a broader folder."
                ),
                ranking_reason=(
                    "Cluster slices rank after direct relationships and keep "
                    "anchor paths ahead of lower-severity members."
                ),
            )
        )
    return slices


def _merge_slices_by_scope(slices: list[dict[str, Any]]) -> list[dict[str, Any]]:
    merged: list[dict[str, Any]] = []
    for slice_payload in slices:
        for existing in merged:
            if existing["scope_paths"] != slice_payload["scope_paths"]:
                continue
            existing["out_of_scope_paths"] = unique_preserving_order(
                existing["out_of_scope_paths"] + slice_payload["out_of_scope_paths"]
            )
            existing["supporting_relationship_ids"] = unique_preserving_order(
                existing["supporting_relationship_ids"]
                + slice_payload["supporting_relationship_ids"]
            )
            existing["supporting_cluster_ids"] = unique_preserving_order(
                existing["supporting_cluster_ids"] + slice_payload["supporting_cluster_ids"]
            )
            break
        else:
            merged.append(slice_payload)
    return merged


def build_plan_payload(
    report: dict[str, Any],
    *,
    path: str | None = None,
    cluster_id: str | None = None,
    relationship_id: str | None = None,
    max_slices: int = 3,
) -> dict[str, Any]:
    selectors = [
        path is not None,
        cluster_id is not None,
        relationship_id is not None,
    ]
    if report.get("schema_version") != EXPECTED_REPORT_SCHEMA_VERSION:
        raise ValueError(
            f"git slop plan requires report schema {EXPECTED_REPORT_SCHEMA_VERSION}."
        )
    if sum(selectors) != 1:
        raise ValueError("Select exactly one of --path, --cluster, or --relationship.")
    if max_slices <= 0:
        raise ValueError("--max-slices must be greater than zero.")
    if path is not None:
        context = _anchor_context_for_path(report, path)
    elif cluster_id is not None:
        context = _anchor_context_for_cluster(report, cluster_id)
    else:
        context = _anchor_context_for_relationship(report, relationship_id or "")

    slices = [_build_anchor_slice(report, context)]
    slices.extend(_build_relationship_slices(report, context))
    slices.extend(_build_cluster_slices(report, context))
    slices = _merge_slices_by_scope(slices)[:max_slices]
    return {
        "schema_version": PLAN_SCHEMA_VERSION,
        "report_schema_version": report.get("schema_version"),
        "command": "plan",
        "selector": context["selector"],
        "target": context["target"],
        "proposed_slices": slices,
        "ranking_basis": {
            "anchor_first": True,
            "relationship_slices_before_cluster_slices": True,
            "max_slice_files": MAX_SLICE_FILES,
            "secondary_sort": "current_hotspot_severity_then_path",
        },
        "boundary_note": BOUNDARY_NOTE,
    }


def render_plan_text(payload: dict[str, Any]) -> str:
    def _render_path_list(paths: list[str]) -> str:
        if not paths:
            return "none"
        preview = paths[:5]
        rendered = ", ".join(preview)
        if len(paths) > len(preview):
            return f"{rendered} (+{len(paths) - len(preview)} more)"
        return rendered

    target = payload["target"]
    if target["kind"] == "path":
        header = f"Plan: path {target['path']} [{target['record_type']}]"
    elif target["kind"] == "cluster":
        header = f"Plan: cluster {target['id']} [{target['cluster_kind']}]"
    else:
        header = f"Plan: relationship {target['id']} [{target['relationship_kind']}]"
    lines = [header]
    for index, slice_payload in enumerate(payload.get("proposed_slices", []), start=1):
        scope = _render_path_list(slice_payload["scope_paths"])
        relationships = ", ".join(slice_payload["supporting_relationship_ids"]) or "none"
        clusters = ", ".join(slice_payload["supporting_cluster_ids"]) or "none"
        out_of_scope = _render_path_list(slice_payload["out_of_scope_paths"])
        lines.extend(
            [
                "",
                f"{index}. {slice_payload['title']}",
                f"   scope: {scope}",
                f"   why: {slice_payload['why_this_slice']}",
                f"   evidence: relationships={relationships}; clusters={clusters}",
                f"   out_of_scope: {out_of_scope}",
            ]
        )
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)
