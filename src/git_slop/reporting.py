from __future__ import annotations

import json
import shutil
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any

import yaml

from .config import latest_dir, runs_dir
from .scoring import CONTEXT_BAND_ORDER, PRIORITY_BAND_ORDER, build_folder_record


def _folder_paths_for_file(path: str) -> list[str]:
    pure_path = PurePosixPath(path)
    parents = ["."]
    current = pure_path.parent
    while str(current) not in ("", "."):
        parents.append(current.as_posix())
        current = current.parent
    return parents


def build_folder_records(
    file_records: list[dict[str, Any]], config: dict[str, Any]
) -> list[dict[str, Any]]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for record in file_records:
        for folder_path in _folder_paths_for_file(record["path"]):
            grouped[folder_path].append(record)
    records = [
        build_folder_record(path=folder_path, descendants=descendants, config=config)
        for folder_path, descendants in grouped.items()
    ]
    return sorted(records, key=lambda record: (record["path"] != ".", record["path"]))


def build_action_queue(
    file_records: list[dict[str, Any]], *, limit: int = 25
) -> list[dict[str, Any]]:
    sorted_records = sorted(
        file_records,
        key=lambda record: (-record["priority_score"], -record["tokens"], record["path"]),
    )
    queue = []
    for record in sorted_records[:limit]:
        queue.append(
            {
                "path": record["path"],
                "priority_score": record["priority_score"],
                "priority_band": record["priority_band"],
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


def _is_pure_context_hotspot(reason_codes: list[str]) -> bool:
    token_cost_reasons = {"high_token_cost", "critical_token_cost"}
    return bool(reason_codes) and set(reason_codes).issubset(token_cost_reasons)


def _signal_label(item: dict[str, Any]) -> str:
    return "context-only" if item["is_pure_context_hotspot"] else "mixed"


def build_report(
    *,
    repo: dict[str, Any],
    config: dict[str, Any],
    file_records: list[dict[str, Any]],
    folder_records: list[dict[str, Any]],
    action_queue: list[dict[str, Any]],
    skipped: dict[str, int],
    generated_at: str,
) -> dict[str, Any]:
    critical_count = sum(1 for record in file_records if record["context_band"] == "critical")
    must_refactor_count = sum(
        1 for record in file_records if record["priority_band"] == "must_refactor"
    )
    files_payload = sorted(
        [{key: value for key, value in record.items() if key != "text"} for record in file_records],
        key=lambda record: record["path"],
    )
    return {
        "schema_version": 1,
        "generated_at": generated_at,
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
            "critical_context_file_count": critical_count,
            "must_refactor_file_count": must_refactor_count,
        },
        "files": files_payload,
        "folders": folder_records,
        "action_queue": action_queue,
    }


def render_terminal_table(action_queue: list[dict[str, Any]]) -> str:
    if not action_queue:
        return "No hotspot records found."
    path_width = max(len("Path"), min(64, max(len(item["path"]) for item in action_queue)))
    header = (
        f"{'Path':<{path_width}}  {'Priority':<14}  {'Context':<8}  "
        f"{'Score':>6}  {'Tokens':>8}  {'Age':>5}  {'Revs':>5}  {'Churn':>6}  {'Signal':<12}"
    )
    lines = [
        header,
        (
            f"{'-' * path_width}  {'-' * 14}  {'-' * 8}  {'-' * 6}  "
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
            f"{item['priority_band']:<14}  "
            f"{item['context_band']:<8}  "
            f"{item['priority_score']:>6.1f}  "
            f"{item['tokens']:>8}  "
            f"{item['age_days']:>5}  "
            f"{item['revisions_window']:>5}  "
            f"{item['churn_pressure']:>6.3f}  "
            f"{_signal_label(item):<12}"
        )
    return "\n".join(lines)


def render_summary(report: dict[str, Any]) -> str:
    lines = [
        "# Git Slop Summary",
        "",
        f"- Repository: `{report['repo']['repo_name']}`",
        f"- Generated at: `{report['generated_at']}`",
        f"- Branch: `{report['repo']['branch'] or 'detached'}`",
        f"- Head commit: `{report['repo']['head_commit'] or 'none'}`",
        f"- Analyzed files: {report['stats']['analyzed_file_count']}",
        f"- Skipped ignored: {report['stats']['skipped_ignored_count']}",
        f"- Skipped missing: {report['stats']['skipped_missing_count']}",
        f"- Skipped binary: {report['stats']['skipped_binary_count']}",
        f"- Skipped undecodable: {report['stats']['skipped_undecodable_count']}",
        f"- Critical context files: {report['stats']['critical_context_file_count']}",
        f"- Must-refactor files: {report['stats']['must_refactor_file_count']}",
        "",
        "## Top Hotspots",
        "",
        "| Path | Priority | Context | Score | Tokens | Age | Revs | Churn | Signal | Reasons |",
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |",
    ]
    for item in report["action_queue"]:
        lines.append(
            f"| `{item['path']}` | `{item['priority_band']}` | `{item['context_band']}` | "
            f"{item['priority_score']:.1f} | {item['tokens']} | {item['age_days']} | "
            f"{item['revisions_window']} | {item['churn_pressure']:.3f} | "
            f"`{_signal_label(item)}` | {', '.join(item['reason_codes']) or '_none_'} |"
        )
    lines.extend(
        [
            "",
            "## Next Action Queue",
            "",
        ]
    )
    if report["action_queue"]:
        for index, item in enumerate(report["action_queue"], start=1):
            lines.append(
                f"{index}. `{item['path']}` "
                f"({item['priority_band']}, {item['context_band']}, "
                f"score {item['priority_score']:.1f}, {item['tokens']} tokens, "
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


def failing_records(
    report: dict[str, Any],
    *,
    fail_on_context_band: str | None,
    fail_on_priority_band: str | None,
) -> list[dict[str, Any]]:
    failures: list[dict[str, Any]] = []
    for record in report["files"]:
        failed = False
        if fail_on_context_band is not None:
            failed = (
                CONTEXT_BAND_ORDER[record["context_band"]]
                >= CONTEXT_BAND_ORDER[fail_on_context_band]
            )
        if fail_on_priority_band is not None:
            failed = failed or (
                PRIORITY_BAND_ORDER[record["priority_band"]]
                >= PRIORITY_BAND_ORDER[fail_on_priority_band]
            )
        if failed:
            failures.append(record)
    return sorted(
        failures,
        key=lambda record: (-record["priority_score"], -record["tokens"], record["path"]),
    )


def utc_timestamp_slug() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
