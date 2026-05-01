from __future__ import annotations

from pathlib import Path
from typing import Any

from git_slop.reports.shared import EXPECTED_REPORT_SCHEMA_VERSION, strongest_pressures

COMPARE_SCHEMA_VERSION = 1
BOUNDARY_NOTE = (
    "Compare boundary: this is a read-only comparison of two existing reports. "
    "It does not rerun the detector, imply causality, mutate repo state, or "
    "change detector scoring semantics."
)
BAND_ORDER = {
    "compact": 0,
    "healthy": 1,
    "warning": 2,
    "low": 0,
    "moderate": 1,
    "high": 2,
    "critical": 3,
}


def _validate_report(report: dict[str, Any], *, label: str) -> None:
    if report.get("schema_version") != EXPECTED_REPORT_SCHEMA_VERSION:
        raise ValueError(f"{label} report must use schema {EXPECTED_REPORT_SCHEMA_VERSION}.")


def _report_descriptor(report: dict[str, Any], path: str | None) -> dict[str, Any]:
    repo = report.get("repo", {}) or {}
    return {
        "path": path,
        "repo_name": repo.get("repo_name"),
        "head_sha": repo.get("head_sha"),
        "generated_at": report.get("generated_at") or report.get("summary", {}).get("generated_at"),
        "schema_version": report.get("schema_version"),
    }


def _records_by_path(report: dict[str, Any], collection: str) -> dict[str, dict[str, Any]]:
    return {
        str(record["path"]): record
        for record in report.get(collection, [])
        if isinstance(record.get("path"), str)
    }


def _score(record: dict[str, Any] | None) -> float | None:
    if record is None:
        return None
    value = record.get("slop_score")
    return round(float(value), 6) if value is not None else None


def _load_pressure(record: dict[str, Any] | None) -> float | None:
    if record is None:
        return None
    costs = record.get("costs", {}) or {}
    load = costs.get("load", {}) or {}
    value = load.get("load_pressure")
    return round(float(value), 6) if value is not None else None


def _token_count(record: dict[str, Any] | None) -> int | None:
    if record is None:
        return None
    costs = record.get("costs", {}) or {}
    load = costs.get("load", {}) or {}
    value = record.get("tokens", load.get("file_token_count"))
    return int(value) if value is not None else None


def _band(record: dict[str, Any] | None, key: str) -> str | None:
    if record is None:
        return None
    value = record.get(key)
    return str(value) if value is not None else None


def _band_delta(base: str | None, head: str | None) -> int | None:
    if base is None or head is None:
        return None
    return BAND_ORDER.get(head, 0) - BAND_ORDER.get(base, 0)


def _overlay_pressures(record: dict[str, Any] | None) -> dict[str, float]:
    if record is None:
        return {}
    return {
        label: round(value, 6)
        for label, value in strongest_pressures(record.get("overlays", {}) or {}, limit=20)
    }


def _overlay_delta(
    base: dict[str, Any] | None,
    head: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    base_pressures = _overlay_pressures(base)
    head_pressures = _overlay_pressures(head)
    changes: list[dict[str, Any]] = []
    for label in sorted(set(base_pressures) | set(head_pressures)):
        base_value = base_pressures.get(label, 0.0)
        head_value = head_pressures.get(label, 0.0)
        delta = round(head_value - base_value, 6)
        if delta == 0.0:
            continue
        changes.append(
            {
                "label": label,
                "base": base_value,
                "head": head_value,
                "delta": delta,
            }
        )
    return sorted(changes, key=lambda item: (-abs(float(item["delta"])), item["label"]))[:5]


def _delta_status(base: dict[str, Any] | None, head: dict[str, Any] | None) -> str:
    if base is None:
        return "added"
    if head is None:
        return "removed"
    if (
        _score(base) == _score(head)
        and _token_count(base) == _token_count(head)
        and _load_pressure(base) == _load_pressure(head)
        and _band(base, "context_band") == _band(head, "context_band")
        and _band(base, "slop_band") == _band(head, "slop_band")
        and not _overlay_delta(base, head)
    ):
        return "unchanged"
    return "changed"


def _record_delta(
    path: str,
    base: dict[str, Any] | None,
    head: dict[str, Any] | None,
) -> dict[str, Any]:
    base_score = _score(base)
    head_score = _score(head)
    base_tokens = _token_count(base)
    head_tokens = _token_count(head)
    base_load = _load_pressure(base)
    head_load = _load_pressure(head)
    base_context = _band(base, "context_band")
    head_context = _band(head, "context_band")
    base_slop = _band(base, "slop_band")
    head_slop = _band(head, "slop_band")
    return {
        "path": path,
        "status": _delta_status(base, head),
        "base_slop_score": base_score,
        "head_slop_score": head_score,
        "slop_score_delta": (
            round((head_score or 0.0) - (base_score or 0.0), 6)
            if base_score is not None or head_score is not None
            else None
        ),
        "base_tokens": base_tokens,
        "head_tokens": head_tokens,
        "token_delta": (
            int((head_tokens or 0) - (base_tokens or 0))
            if base_tokens is not None or head_tokens is not None
            else None
        ),
        "base_load_pressure": base_load,
        "head_load_pressure": head_load,
        "load_pressure_delta": (
            round((head_load or 0.0) - (base_load or 0.0), 6)
            if base_load is not None or head_load is not None
            else None
        ),
        "base_context_band": base_context,
        "head_context_band": head_context,
        "context_band_delta": _band_delta(base_context, head_context),
        "base_slop_band": base_slop,
        "head_slop_band": head_slop,
        "slop_band_delta": _band_delta(base_slop, head_slop),
        "overlay_deltas": _overlay_delta(base, head),
    }


def _record_deltas(
    base_report: dict[str, Any],
    head_report: dict[str, Any],
    collection: str,
) -> list[dict[str, Any]]:
    base_records = _records_by_path(base_report, collection)
    head_records = _records_by_path(head_report, collection)
    return [
        _record_delta(path, base_records.get(path), head_records.get(path))
        for path in sorted(set(base_records) | set(head_records))
    ]


def _queue_positions(report: dict[str, Any]) -> dict[str, int]:
    positions: dict[str, int] = {}
    for index, item in enumerate(report.get("action_queue", []), start=1):
        path = item.get("path")
        if isinstance(path, str) and path not in positions:
            positions[path] = index
    return positions


def _queue_movement(
    base_report: dict[str, Any],
    head_report: dict[str, Any],
) -> list[dict[str, Any]]:
    base_positions = _queue_positions(base_report)
    head_positions = _queue_positions(head_report)
    movements: list[dict[str, Any]] = []
    for path in sorted(set(base_positions) | set(head_positions)):
        base_position = base_positions.get(path)
        head_position = head_positions.get(path)
        if base_position is None:
            status = "newly_queued"
            position_delta = None
        elif head_position is None:
            status = "dropped_from_queue"
            position_delta = None
        else:
            position_delta = head_position - base_position
            if position_delta < 0:
                status = "moved_up"
            elif position_delta > 0:
                status = "moved_down"
            else:
                status = "unchanged_position"
        movements.append(
            {
                "path": path,
                "status": status,
                "base_position": base_position,
                "head_position": head_position,
                "position_delta": position_delta,
            }
        )
    return sorted(
        movements,
        key=lambda item: (
            {
                "newly_queued": 0,
                "moved_up": 1,
                "moved_down": 2,
                "dropped_from_queue": 3,
                "unchanged_position": 4,
            }[item["status"]],
            item["head_position"] or item["base_position"] or 10_000,
            item["path"],
        ),
    )


def _summary(
    file_deltas: list[dict[str, Any]],
    folder_deltas: list[dict[str, Any]],
) -> dict[str, Any]:
    def _counts(items: list[dict[str, Any]]) -> dict[str, int]:
        return {
            status: sum(1 for item in items if item["status"] == status)
            for status in ("added", "removed", "changed", "unchanged")
        }

    return {
        "files": _counts(file_deltas),
        "folders": _counts(folder_deltas),
        "worsened_file_count": sum(
            1 for item in file_deltas if float(item.get("slop_score_delta") or 0.0) > 0.0
        ),
        "improved_file_count": sum(
            1 for item in file_deltas if float(item.get("slop_score_delta") or 0.0) < 0.0
        ),
    }


def _overlay_deltas(file_deltas: list[dict[str, Any]]) -> list[dict[str, Any]]:
    aggregate: dict[str, float] = {}
    for item in file_deltas:
        for overlay in item.get("overlay_deltas", []):
            aggregate[overlay["label"]] = round(
                aggregate.get(overlay["label"], 0.0) + float(overlay["delta"]),
                6,
            )
    return [
        {"label": label, "total_delta": delta}
        for label, delta in sorted(aggregate.items(), key=lambda item: (-abs(item[1]), item[0]))
        if delta != 0.0
    ]


def build_compare_payload(
    base_report: dict[str, Any],
    head_report: dict[str, Any],
    *,
    base_path: str | None = None,
    head_path: str | None = None,
    top: int = 10,
) -> dict[str, Any]:
    _validate_report(base_report, label="base")
    _validate_report(head_report, label="head")
    if top <= 0:
        raise ValueError("--top must be greater than zero.")

    file_deltas = _record_deltas(base_report, head_report, "files")
    folder_deltas = _record_deltas(base_report, head_report, "folders")
    return {
        "schema_version": COMPARE_SCHEMA_VERSION,
        "report_schema_version": EXPECTED_REPORT_SCHEMA_VERSION,
        "command": "compare",
        "base_report": _report_descriptor(base_report, base_path),
        "head_report": _report_descriptor(head_report, head_path),
        "summary": _summary(file_deltas, folder_deltas),
        "file_deltas": file_deltas,
        "folder_deltas": folder_deltas,
        "queue_movement": _queue_movement(base_report, head_report)[:top],
        "overlay_deltas": _overlay_deltas(file_deltas),
        "boundary_note": BOUNDARY_NOTE,
    }


def _rank_by_score_delta(items: list[dict[str, Any]], *, reverse: bool) -> list[dict[str, Any]]:
    return sorted(
        [
            item
            for item in items
            if item["status"] != "unchanged"
            and item.get("slop_score_delta") is not None
            and float(item["slop_score_delta"]) != 0.0
        ],
        key=lambda item: (
            (
                -float(item["slop_score_delta"])
                if reverse
                else float(item["slop_score_delta"])
            ),
            item["path"],
        ),
    )


def render_compare_text(payload: dict[str, Any], *, top: int = 10) -> str:
    base_path = payload.get("base_report", {}).get("path") or "<base>"
    head_path = payload.get("head_report", {}).get("path") or "<head>"
    summary = payload.get("summary", {})
    file_counts = summary.get("files", {})
    folder_counts = summary.get("folders", {})
    lines = [
        f"Compare: {Path(base_path).name} -> {Path(head_path).name}",
        "",
        "Summary",
        (
            "- files: "
            f"added={file_counts.get('added', 0)}, "
            f"removed={file_counts.get('removed', 0)}, "
            f"changed={file_counts.get('changed', 0)}, "
            f"unchanged={file_counts.get('unchanged', 0)}"
        ),
        (
            "- folders: "
            f"added={folder_counts.get('added', 0)}, "
            f"removed={folder_counts.get('removed', 0)}, "
            f"changed={folder_counts.get('changed', 0)}, "
            f"unchanged={folder_counts.get('unchanged', 0)}"
        ),
        (
            "- slop score movement: "
            f"worsened_files={summary.get('worsened_file_count', 0)}, "
            f"improved_files={summary.get('improved_file_count', 0)}"
        ),
        "",
        "Top Worsened Files",
    ]
    worsened = _rank_by_score_delta(payload.get("file_deltas", []), reverse=True)[:top]
    lines.extend(
        [
            (
                f"- {item['path']}: "
                f"{item['base_slop_score']} -> {item['head_slop_score']} "
                f"(delta={item['slop_score_delta']})"
            )
            for item in worsened
        ]
        or ["- none"]
    )
    lines.extend(["", "Top Improved Files"])
    improved = _rank_by_score_delta(payload.get("file_deltas", []), reverse=False)[:top]
    lines.extend(
        [
            (
                f"- {item['path']}: "
                f"{item['base_slop_score']} -> {item['head_slop_score']} "
                f"(delta={item['slop_score_delta']})"
            )
            for item in improved
        ]
        or ["- none"]
    )
    lines.extend(["", "Queue Movement"])
    lines.extend(
        [
            (
                f"- {item['path']}: {item['status']} "
                f"base={item['base_position']} head={item['head_position']}"
            )
            for item in payload.get("queue_movement", [])[:top]
        ]
        or ["- none"]
    )
    lines.extend(["", payload["boundary_note"]])
    return "\n".join(lines)
