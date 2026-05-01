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

PLAN_SCHEMA_VERSION = 2
MAX_SLICE_FILES = 5
BOUNDARY_NOTE = (
    "Plan boundary: this is a bounded proposal only. It does not mutate code, "
    "GitHub, or detector truth, and it does not guarantee correctness or safety."
)
COMPACT_CLUSTER_KINDS = {"duplicate_set", "consolidation_candidate"}
COMPACT_CLUSTER_CANDIDATES = {
    "duplicate_set",
    "consolidation_candidate",
    "consolidate_duplicate_knowledge",
}


def _slug(value: str) -> str:
    return (
        value.replace("/", "--")
        .replace(".", "_")
        .replace(":", "_")
        .replace(" ", "-")
    )


def _record_slop_score(report: dict[str, Any], path: str) -> float:
    record = resolve_path(report, path)
    return float((record or {}).get("slop_score") or 0.0)


def _sort_paths_by_severity(
    report: dict[str, Any],
    paths: list[str],
    *,
    anchor_paths: list[str],
) -> list[str]:
    path_records = {path: resolve_path(report, path) for path in dict.fromkeys(paths)}
    anchor_order = {path: index for index, path in enumerate(anchor_paths)}
    sorted_paths = [
        path
        for path, _record in sorted(
            path_records.items(),
            key=lambda item: (
                0 if item[0] in anchor_order else 1,
                anchor_order.get(item[0], 10_000),
                -float((item[1] or {}).get("slop_score") or 0.0),
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


def _build_path_target(record: dict[str, Any]) -> dict[str, Any]:
    return {
        "kind": "path",
        "path": record["path"],
        "record_type": record["record_type"],
        "slop_score": record.get("slop_score"),
        "slop_band": record.get("slop_band"),
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
    shared_clusters = rank_clusters(
        shared_clusters_for_relationship(report, relationship),
        anchor_paths=[relationship["source_path"], relationship["target_path"]],
    )
    return {
        "selector": {"kind": "relationship", "value": relationship_id},
        "target": _build_relationship_target(relationship),
        "anchor_paths": [relationship["source_path"], relationship["target_path"]],
        "supporting_relationships": [relationship],
        "supporting_clusters": shared_clusters,
        "focus_folder": None,
        "relationship": relationship,
    }


def _selector_root(path: str) -> str:
    return path.split("/", 1)[0]


def _cluster_same_root_anchor_paths(report: dict[str, Any], cluster: dict[str, Any]) -> list[str]:
    groups: dict[str, list[str]] = {}
    for path in cluster["member_paths"]:
        if resolve_path(report, path) is None:
            continue
        groups.setdefault(_selector_root(path), []).append(path)
    if not groups:
        return []
    for root, paths in groups.items():
        groups[root] = _sort_paths_by_severity(report, paths, anchor_paths=[])

    multi_member_roots = [root for root, paths in groups.items() if len(paths) >= 2]
    candidate_roots = multi_member_roots or list(groups)
    ranked_roots = sorted(
        candidate_roots,
        key=lambda root: (
            -sum(_record_slop_score(report, path) for path in groups[root][:2]),
            -_record_slop_score(report, groups[root][0]),
            root,
        ),
    )
    selected = groups[ranked_roots[0]]
    return selected[:3]


def _cluster_anchor_candidate_paths(report: dict[str, Any], context: dict[str, Any]) -> list[str]:
    target = context["target"]
    if context["selector"]["kind"] != "cluster":
        return list(context["anchor_paths"])
    if target["member_count"] <= MAX_SLICE_FILES and len(target.get("top_level_roots", [])) <= 2:
        return list(context["anchor_paths"])
    if context["supporting_relationships"]:
        leading_relationship = context["supporting_relationships"][0]
        return [leading_relationship["source_path"], leading_relationship["target_path"]]
    same_root = _cluster_same_root_anchor_paths(report, context["cluster"])
    if same_root:
        return same_root
    return list(context["anchor_paths"])[:3]


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
    selector_class: int,
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
        "_selector_class": selector_class,
    }


def _priority_hint_for_slice(report: dict[str, Any], scope_paths: list[str]) -> str:
    top_score = max((_record_slop_score(report, path) for path in scope_paths), default=0.0)
    if top_score >= 75.0:
        return "Now"
    if top_score >= 40.0:
        return "Next"
    return "Later"


def _issue_title_for_slice(slice_payload: dict[str, Any]) -> str:
    return f"Maintenance: {slice_payload['title']}"


def _evidence_summary_for_slice(slice_payload: dict[str, Any]) -> str:
    relationships = slice_payload["supporting_relationship_ids"]
    clusters = slice_payload["supporting_cluster_ids"]
    evidence_parts = []
    if relationships:
        evidence_parts.append(f"{len(relationships)} relationship(s)")
    if clusters:
        evidence_parts.append(f"{len(clusters)} cluster(s)")
    if not evidence_parts:
        evidence_parts.append("anchor detector evidence")
    return (
        f"{slice_payload['why_this_slice']} Evidence: "
        f"{', '.join(evidence_parts)}. Scope: "
        f"{', '.join(slice_payload['scope_paths']) or 'none'}."
    )


def _backlog_handoff_for_slice(
    report: dict[str, Any],
    context: dict[str, Any],
    slice_payload: dict[str, Any],
) -> dict[str, Any]:
    return {
        "mutation_policy": "preview_only",
        "proposed_issue_title": _issue_title_for_slice(slice_payload),
        "issue_type": "maintenance",
        "suggested_labels": ["maintenance"],
        "priority_hint": _priority_hint_for_slice(report, slice_payload["scope_paths"]),
        "evidence_summary": _evidence_summary_for_slice(slice_payload),
        "acceptance_criteria": [
            "Review the scoped paths against the cited git-slop evidence.",
            "Keep changes bounded to the proposed scope unless new evidence is documented.",
            "Preserve detector score, check, and overlay semantics.",
        ],
        "source": {
            "command": "git slop plan",
            "selector": context["selector"],
            "report_schema_version": report.get("schema_version"),
        },
    }


def _enrich_public_slice(
    report: dict[str, Any],
    context: dict[str, Any],
    slice_payload: dict[str, Any],
) -> dict[str, Any]:
    public = _public_slice(slice_payload)
    public["evidence_summary"] = _evidence_summary_for_slice(public)
    public["backlog_handoff"] = _backlog_handoff_for_slice(report, context, public)
    return public


def _combined_cluster_out_of_scope(
    cluster: dict[str, Any],
    scope_paths: list[str],
    out_of_scope_paths: list[str],
) -> list[str]:
    omitted_paths = [path for path in cluster["member_paths"] if path not in scope_paths]
    return unique_preserving_order(out_of_scope_paths + omitted_paths)


def _count_descendants(scope_paths: list[str], folder_path: str | None) -> int:
    if folder_path is None:
        return 0
    prefix = f"{folder_path.rstrip('/')}/"
    return sum(1 for path in scope_paths if path.startswith(prefix))


def _cluster_bounded_density(cluster: dict[str, Any], scope_paths: list[str]) -> float:
    member_count = max(int(cluster.get("member_count") or 0), 1)
    return len(scope_paths) / member_count


def _cluster_qualifies(
    cluster: dict[str, Any],
    scope_paths: list[str],
    *,
    selector_kind: str,
    descendant_count: int = 0,
) -> bool:
    if int(cluster.get("member_count") or 0) <= 8:
        return True
    if _cluster_bounded_density(cluster, scope_paths) >= 0.5:
        return True
    if cluster.get("kind") in COMPACT_CLUSTER_KINDS:
        return True
    if cluster.get("candidate_type") in COMPACT_CLUSTER_CANDIDATES:
        return True
    if selector_kind == "path" and descendant_count >= 2:
        return True
    return False


def _cluster_selector_class(cluster: dict[str, Any], out_of_scope_paths: list[str]) -> int:
    if (
        int(cluster.get("member_count") or 0) <= 8
        or cluster.get("kind") in COMPACT_CLUSTER_KINDS
        or cluster.get("candidate_type") in COMPACT_CLUSTER_CANDIDATES
    ) and not out_of_scope_paths:
        return 2
    return 3


def _build_anchor_slice(report: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    target = context["target"]
    selector_kind = context["selector"]["kind"]
    candidate_paths = _cluster_anchor_candidate_paths(report, context)
    scope_paths, out_of_scope_paths = _build_scope(
        report,
        candidate_paths=candidate_paths,
        anchor_paths=candidate_paths,
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
        if candidate_paths != list(context["anchor_paths"]):
            title = f"Start inside cluster {target['id']}"
            why = (
                "The selected cluster is broad, so start with the strongest "
                "reviewable sub-slice before expanding."
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
        ranking_reason="Anchor slice always ranks first.",
        selector_class=0,
    )


def _relationship_slice(
    report: dict[str, Any],
    context: dict[str, Any],
    relationship: dict[str, Any],
) -> dict[str, Any] | None:
    focus_folder = context.get("focus_folder")
    candidate_paths = [relationship["source_path"], relationship["target_path"]]
    if focus_folder is not None:
        folder_prefix = f"{focus_folder.rstrip('/')}/"
        descendants = [path for path in candidate_paths if path.startswith(folder_prefix)]
        external = [path for path in candidate_paths if path not in descendants]
        if not descendants:
            return None
        candidate_paths = descendants + external[:1]
    scope_paths, out_of_scope_paths = _build_scope(
        report,
        candidate_paths=candidate_paths,
        anchor_paths=context["anchor_paths"],
    )
    if len(scope_paths) < 2:
        return None
    if focus_folder is not None:
        descendant_count = _count_descendants(scope_paths, focus_folder)
        external_count = len(scope_paths) - descendant_count
        if not (
            descendant_count >= 2 or (descendant_count == 1 and external_count >= 1)
        ):
            return None
    shared_clusters = shared_clusters_for_relationship(report, relationship)
    return _slice_payload(
        slice_id=f"relationship-{relationship['id']}",
        title=f"Inspect relationship {relationship['id']}",
        scope_paths=scope_paths,
        out_of_scope_paths=out_of_scope_paths,
        supporting_relationship_ids=[relationship["id"]],
        supporting_cluster_ids=[cluster["id"] for cluster in shared_clusters[:3]],
        why_this_slice=(
            "This pair already co-occurs in direct detector evidence and should be "
            "reviewed together."
        ),
        ranking_reason="Direct relationship slices rank immediately after the anchor slice.",
        selector_class=1,
    )


def _cluster_slice(
    report: dict[str, Any],
    context: dict[str, Any],
    cluster: dict[str, Any],
) -> dict[str, Any] | None:
    focus_folder = context.get("focus_folder")
    if focus_folder is not None:
        candidate_paths = _folder_cluster_scope_paths(
            report,
            folder_path=focus_folder,
            cluster=cluster,
        )
    else:
        candidate_paths = list(cluster["member_paths"])
    scope_paths, out_of_scope_paths = _build_scope(
        report,
        candidate_paths=candidate_paths,
        anchor_paths=context["anchor_paths"],
    )
    combined_out_of_scope = _combined_cluster_out_of_scope(
        cluster,
        scope_paths,
        out_of_scope_paths,
    )
    descendant_count = _count_descendants(scope_paths, focus_folder)
    if focus_folder is not None:
        external_count = len(scope_paths) - descendant_count
        if not (
            descendant_count >= 2 or (descendant_count == 1 and external_count >= 1)
        ):
            return None
    if not _cluster_qualifies(
        cluster,
        scope_paths,
        selector_kind=context["selector"]["kind"],
        descendant_count=descendant_count,
    ):
        return None
    if context["selector"]["kind"] == "path" and context["target"]["record_type"] == "file":
        anchor_set = set(context["anchor_paths"])
        if not any(path not in anchor_set for path in scope_paths):
            return None
    if context["selector"]["kind"] == "relationship":
        anchor_set = set(context["anchor_paths"])
        non_anchor_scope = [path for path in scope_paths if path not in anchor_set]
        if not non_anchor_scope:
            return None
        if len(combined_out_of_scope) > len(scope_paths):
            return None
    return _slice_payload(
        slice_id=f"cluster-{cluster['id']}",
        title=f"Inspect cluster {cluster['id']}",
        scope_paths=scope_paths,
        out_of_scope_paths=combined_out_of_scope,
        supporting_relationship_ids=cluster.get("source_relationship_ids", [])[:3],
        supporting_cluster_ids=[cluster["id"]],
        why_this_slice=(
            "This slice stays inside one direct structural cluster instead of "
            "sweeping a broader folder."
        ),
        ranking_reason="Cluster slices rank after direct relationship slices.",
        selector_class=_cluster_selector_class(cluster, combined_out_of_scope),
    )


def _build_file_path_candidates(
    report: dict[str, Any],
    context: dict[str, Any],
) -> list[dict[str, Any]]:
    slices = [_build_anchor_slice(report, context)]
    for relationship in context["supporting_relationships"][:3]:
        slice_payload = _relationship_slice(report, context, relationship)
        if slice_payload is not None:
            slices.append(slice_payload)
    emitted_clusters = 0
    for cluster in context["supporting_clusters"]:
        slice_payload = _cluster_slice(report, context, cluster)
        if slice_payload is None:
            continue
        slices.append(slice_payload)
        emitted_clusters += 1
        if emitted_clusters >= 2:
            break
    return slices


def _build_folder_path_candidates(
    report: dict[str, Any],
    context: dict[str, Any],
) -> list[dict[str, Any]]:
    slices = [_build_anchor_slice(report, context)]
    for relationship in context["supporting_relationships"]:
        slice_payload = _relationship_slice(report, context, relationship)
        if slice_payload is not None:
            slices.append(slice_payload)
    for cluster in context["supporting_clusters"]:
        slice_payload = _cluster_slice(report, context, cluster)
        if slice_payload is not None:
            slices.append(slice_payload)
    return slices


def _build_relationship_candidates(
    report: dict[str, Any],
    context: dict[str, Any],
) -> list[dict[str, Any]]:
    slices = [_build_anchor_slice(report, context)]
    emitted_followups = 0
    for cluster in context["supporting_clusters"]:
        slice_payload = _cluster_slice(report, context, cluster)
        if slice_payload is None:
            continue
        slices.append(slice_payload)
        emitted_followups += 1
        if emitted_followups >= 2:
            break
    return slices


def _build_cluster_candidates(
    report: dict[str, Any],
    context: dict[str, Any],
) -> list[dict[str, Any]]:
    slices = [_build_anchor_slice(report, context)]
    emitted_followups = 0
    for relationship in context["supporting_relationships"]:
        slice_payload = _relationship_slice(report, context, relationship)
        if slice_payload is None:
            continue
        slices.append(slice_payload)
        emitted_followups += 1
        if emitted_followups >= 2:
            break
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
            if slice_payload["_selector_class"] < existing["_selector_class"]:
                existing["_selector_class"] = slice_payload["_selector_class"]
                existing["id"] = slice_payload["id"]
                existing["title"] = slice_payload["title"]
                existing["why_this_slice"] = slice_payload["why_this_slice"]
                existing["ranking_reason"] = slice_payload["ranking_reason"]
            break
        else:
            merged.append(slice_payload)
    return merged


def _top_slop_score_sum(report: dict[str, Any], paths: list[str]) -> float:
    scores = sorted(
        (_record_slop_score(report, path) for path in paths),
        reverse=True,
    )
    return sum(scores[:3])


def _slice_rank(
    report: dict[str, Any],
    slice_payload: dict[str, Any],
    *,
    selector_kind: str,
) -> tuple[Any, ...]:
    if selector_kind == "path":
        return (
            0 if slice_payload["_selector_class"] == 0 else 1,
            -_top_slop_score_sum(report, slice_payload["scope_paths"]),
            len(slice_payload["out_of_scope_paths"]),
            tuple(slice_payload["scope_paths"]),
        )
    return (
        slice_payload["_selector_class"],
        -len(slice_payload["supporting_relationship_ids"]),
        -len(slice_payload["supporting_cluster_ids"]),
        len(slice_payload["out_of_scope_paths"]),
        -_top_slop_score_sum(report, slice_payload["scope_paths"]),
        tuple(slice_payload["scope_paths"]),
    )


def _suppress_weaker_subsets(
    ranked_slices: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    kept: list[dict[str, Any]] = []
    for slice_payload in ranked_slices:
        current_scope = set(slice_payload["scope_paths"])
        current_relationships = set(slice_payload["supporting_relationship_ids"])
        current_clusters = set(slice_payload["supporting_cluster_ids"])
        suppress = False
        for existing in kept:
            existing_scope = set(existing["scope_paths"])
            if not current_scope < existing_scope:
                continue
            if current_relationships - set(existing["supporting_relationship_ids"]):
                continue
            if current_clusters - set(existing["supporting_cluster_ids"]):
                continue
            suppress = True
            break
        if not suppress:
            kept.append(slice_payload)
    return kept


def _public_slice(slice_payload: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in slice_payload.items() if not key.startswith("_")}


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
        if context["target"]["record_type"] == "folder":
            slices = _build_folder_path_candidates(report, context)
        else:
            slices = _build_file_path_candidates(report, context)
    elif cluster_id is not None:
        context = _anchor_context_for_cluster(report, cluster_id)
        slices = _build_cluster_candidates(report, context)
    else:
        context = _anchor_context_for_relationship(report, relationship_id or "")
        slices = _build_relationship_candidates(report, context)

    merged = _merge_slices_by_scope(slices)
    ranked = sorted(
        merged,
        key=lambda item: _slice_rank(
            report,
            item,
            selector_kind=context["selector"]["kind"],
        ),
    )
    suppressed = _suppress_weaker_subsets(ranked)
    public_slices = [
        _enrich_public_slice(report, context, item)
        for item in suppressed[:max_slices]
    ]

    return {
        "schema_version": PLAN_SCHEMA_VERSION,
        "report_schema_version": report.get("schema_version"),
        "command": "plan",
        "selector": context["selector"],
        "target": context["target"],
        "proposed_slices": public_slices,
        "ranking_basis": {
            "anchor_first": True,
            "relationship_slices_before_cluster_slices": context["selector"]["kind"] != "path",
            "max_slice_files": MAX_SLICE_FILES,
            "secondary_sort": (
                "top-three-slop-score-sum, out-of-scope-count, path"
                if context["selector"]["kind"] == "path"
                else (
                    "relationship-count, cluster-count, out-of-scope-count, "
                    "top-three-slop-score-sum, path"
                )
            ),
        },
        "backlog_handoff": {
            "mutation_policy": "preview_only",
            "candidate_count": len(public_slices),
            "target_plugin_skill": "$project-management-workflows:plan-to-backlog-preview",
            "source_selector": context["selector"],
        },
        "boundary_note": BOUNDARY_NOTE,
    }


def render_plan_text(payload: dict[str, Any]) -> str:
    def _render_scope(paths: list[str]) -> str:
        return ", ".join(paths) if paths else "none"

    def _render_limited_paths(paths: list[str], limit: int = 5) -> str:
        if not paths:
            return "none"
        preview = paths[:limit]
        rendered = ", ".join(preview)
        if len(paths) > len(preview):
            return f"{rendered} (+{len(paths) - len(preview)} more)"
        return rendered

    def _render_ids(items: list[str], *, limit: int) -> str:
        if not items:
            return "none"
        preview = items[:limit]
        rendered = ", ".join(preview)
        if len(items) > len(preview):
            return f"{rendered} (+{len(items) - len(preview)} more)"
        return rendered

    def _render_backlog(slice_payload: dict[str, Any]) -> str:
        handoff = slice_payload.get("backlog_handoff", {})
        title = handoff.get("proposed_issue_title", "n/a")
        priority = handoff.get("priority_hint", "n/a")
        return f"   backlog: {title} priority={priority} policy=preview_only"

    target = payload["target"]
    if target["kind"] == "path":
        header = f"Plan: path {target['path']} [{target['record_type']}]"
    elif target["kind"] == "cluster":
        header = f"Plan: cluster {target['id']} [{target['cluster_kind']}]"
    else:
        header = f"Plan: relationship {target['id']} [{target['relationship_kind']}]"
    lines = [header]
    for index, slice_payload in enumerate(payload.get("proposed_slices", []), start=1):
        scope = _render_scope(slice_payload["scope_paths"])
        relationships = _render_ids(
            slice_payload["supporting_relationship_ids"],
            limit=3,
        )
        clusters = _render_ids(slice_payload["supporting_cluster_ids"], limit=2)
        out_of_scope = _render_limited_paths(slice_payload["out_of_scope_paths"], limit=5)
        lines.extend(
            [
                "",
                f"{index}. {slice_payload['title']}",
                f"   scope: {scope}",
                f"   why: {slice_payload['why_this_slice']}",
                f"   evidence_summary: {slice_payload.get('evidence_summary', 'n/a')}",
                f"   evidence: relationships={relationships}; clusters={clusters}",
                _render_backlog(slice_payload),
                f"   out_of_scope: {out_of_scope}",
            ]
        )
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)
