from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from git_slop.reports.shared import (
    EXPECTED_REPORT_SCHEMA_VERSION,
    resolve_path,
    strongest_pressures,
)

SARIF_SCHEMA_VERSION = 1
SARIF_VERSION = "2.1.0"
SARIF_SCHEMA_URL = "https://json.schemastore.org/sarif-2.1.0.json"
BOUNDARY_NOTE = (
    "SARIF export boundary: this is a deterministic projection of existing "
    "git-slop report evidence. It does not rerun the detector, upload results, "
    "mutate code, or change detector scoring semantics."
)
RULE_ID = "git-slop.hotspot"


def _validate_report(report: dict[str, Any]) -> None:
    if report.get("schema_version") != EXPECTED_REPORT_SCHEMA_VERSION:
        raise ValueError(f"git slop sarif requires report schema {EXPECTED_REPORT_SCHEMA_VERSION}.")


def _level_for_record(record: dict[str, Any]) -> str:
    priority_band = record.get("priority_band")
    context_band = record.get("context_band")
    if priority_band == "must_refactor" or context_band == "critical":
        return "error"
    if priority_band in {"should_refactor", "needs_refactor"} or context_band == "warning":
        return "warning"
    return "note"


def _record_for_queue_item(report: dict[str, Any], item: dict[str, Any]) -> dict[str, Any]:
    record = resolve_path(report, item["path"])
    if record is None:
        return dict(item)
    payload = dict(record)
    payload.setdefault("priority_score", item.get("priority_score"))
    payload.setdefault("priority_band", item.get("priority_band"))
    payload.setdefault("context_band", item.get("context_band"))
    payload.setdefault("reason_codes", item.get("reason_codes", []))
    return payload


def _message_for_record(record: dict[str, Any]) -> str:
    reason_codes = record.get("reason_codes") or []
    reasons = ", ".join(reason_codes) if reason_codes else "no reason codes"
    return (
        f"{record.get('path')} is ranked {record.get('priority_band')} "
        f"with score {record.get('priority_score')} and context "
        f"{record.get('context_band')} ({reasons})."
    )


def _overlay_properties(record: dict[str, Any]) -> dict[str, float]:
    return {
        label: round(value, 6)
        for label, value in strongest_pressures(record.get("overlays", {}) or {}, limit=8)
    }


def _result_for_record(record: dict[str, Any], *, rank: int) -> dict[str, Any]:
    path = str(record.get("path"))
    return {
        "ruleId": RULE_ID,
        "ruleIndex": 0,
        "level": _level_for_record(record),
        "message": {"text": _message_for_record(record)},
        "locations": [
            {
                "physicalLocation": {
                    "artifactLocation": {"uri": path},
                },
            }
        ],
        "properties": {
            "git_slop": {
                "rank": rank,
                "priority_score": record.get("priority_score"),
                "priority_band": record.get("priority_band"),
                "context_band": record.get("context_band"),
                "reason_codes": record.get("reason_codes", []),
                "costs": record.get("costs", {}),
                "strongest_overlays": _overlay_properties(record),
                "evidence_boundary": (
                    "Hotspot cost and overlay evidence are preserved as separate "
                    "properties; SARIF export does not rescore the finding."
                ),
            }
        },
    }


def _rules() -> list[dict[str, Any]]:
    return [
        {
            "id": RULE_ID,
            "name": "Git Slop hotspot",
            "shortDescription": {
                "text": "File ranked in the git-slop action queue.",
            },
            "fullDescription": {
                "text": (
                    "A deterministic git-slop hotspot based on context cost. "
                    "Overlay evidence is exported separately in result properties "
                    "and does not change detector scoring."
                ),
            },
            "help": {
                "text": (
                    "Review the git-slop report, explain output, or plan output for "
                    "supporting evidence before deciding whether maintenance work is "
                    "appropriate."
                ),
            },
            "properties": {
                "precision": "medium",
                "tags": ["maintainability", "context-cost", "git-slop"],
            },
        }
    ]


def build_sarif_payload(
    report: dict[str, Any],
    *,
    report_path: str | None = None,
    top: int | None = None,
) -> dict[str, Any]:
    _validate_report(report)
    if top is not None and top <= 0:
        raise ValueError("--top must be greater than zero.")

    action_queue = report.get("action_queue", [])
    selected = action_queue[:top] if top is not None else action_queue
    results = [
        _result_for_record(_record_for_queue_item(report, item), rank=index)
        for index, item in enumerate(selected, start=1)
        if isinstance(item.get("path"), str)
    ]
    repo = report.get("repo", {}) or {}
    return {
        "$schema": SARIF_SCHEMA_URL,
        "version": SARIF_VERSION,
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "git-slop",
                        "informationUri": "https://github.com/coreycoto/git-slop",
                        "rules": _rules(),
                    }
                },
                "automationDetails": {
                    "id": "git-slop/sarif",
                },
                "versionControlProvenance": [
                    {
                        "repositoryUri": repo.get("remote_url") or repo.get("repo_name"),
                        "revisionId": repo.get("head_sha"),
                    }
                ],
                "invocations": [
                    {
                        "executionSuccessful": True,
                        "properties": {
                            "git_slop": {
                                "schema_version": SARIF_SCHEMA_VERSION,
                                "report_schema_version": report.get("schema_version"),
                                "report_path": report_path,
                                "boundary_note": BOUNDARY_NOTE,
                            }
                        },
                    }
                ],
                "results": results,
                "properties": {
                    "git_slop": {
                        "summary": report.get("summary", {}),
                        "stats": report.get("stats", {}),
                        "boundary_note": BOUNDARY_NOTE,
                    }
                },
            }
        ],
    }


def render_sarif_json(payload: dict[str, Any]) -> str:
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def write_sarif_file(payload: dict[str, Any], output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(render_sarif_json(payload), encoding="utf-8")
