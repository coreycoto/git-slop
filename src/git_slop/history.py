from __future__ import annotations

from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from .git import has_head_commit, run_git


def _parse_unix_timestamp(raw_value: str) -> int | None:
    try:
        return int(raw_value.strip())
    except (TypeError, ValueError):
        return None


def _age_days_from_timestamp(first_seen_timestamp: int | None, *, now: datetime) -> int:
    if first_seen_timestamp is None:
        return 0
    first_seen = datetime.fromtimestamp(first_seen_timestamp, tz=timezone.utc)
    delta = now - first_seen
    return max(0, int(delta.total_seconds() // 86400))


def build_history_metrics(
    repo_root: Path,
    records: list[dict[str, Any]],
    config: dict[str, Any],
) -> dict[str, dict[str, float | int]]:
    empty_metrics = {
        record["path"]: {
            "age_days": 0,
            "revisions_window": 0,
            "added_window": 0,
            "deleted_window": 0,
            "churn_lines_window": 0,
            "relative_churn_window": 0.0,
        }
        for record in records
    }
    if not records or not has_head_commit(repo_root):
        return empty_metrics

    now = datetime.now(timezone.utc)
    history_config = config["history"]
    window_days = int(history_config["churn_window_days"])
    follow_renames = bool(history_config["follow_renames"])
    since_utc = (now - timedelta(days=window_days)).strftime("%Y-%m-%dT%H:%M:%SZ")
    tracked_paths = {record["path"] for record in records}
    metrics = defaultdict(
        lambda: {
            "age_days": 0,
            "revisions_window": 0,
            "added_window": 0,
            "deleted_window": 0,
            "churn_lines_window": 0,
            "relative_churn_window": 0.0,
        }
    )

    log_args = ["log", "--numstat", "--format=commit:%H", f"--since={since_utc}"]
    if not follow_renames:
        log_args.append("--no-renames")
    completed = run_git(repo_root, log_args)
    if completed.returncode == 0:
        for line in completed.stdout.splitlines():
            if not line or line.startswith("commit:"):
                continue
            parts = line.split("\t", 2)
            if len(parts) != 3:
                continue
            added_raw, deleted_raw, path = parts
            normalized_path = Path(path).as_posix()
            if normalized_path not in tracked_paths:
                continue
            if added_raw == "-" or deleted_raw == "-":
                continue
            added = int(added_raw)
            deleted = int(deleted_raw)
            metrics[normalized_path]["revisions_window"] += 1
            metrics[normalized_path]["added_window"] += added
            metrics[normalized_path]["deleted_window"] += deleted
            metrics[normalized_path]["churn_lines_window"] += added + deleted

    for record in records:
        age_args = ["log", "--diff-filter=A", "--format=%ct", "--reverse"]
        if follow_renames:
            age_args.append("--follow")
        age_args.extend(["--", record["path"]])
        completed = run_git(repo_root, age_args)
        first_seen_timestamp = None
        if completed.returncode == 0:
            lines = [line for line in completed.stdout.splitlines() if line.strip()]
            if lines:
                first_seen_timestamp = _parse_unix_timestamp(lines[0])
        age_days = _age_days_from_timestamp(first_seen_timestamp, now=now)
        churn_lines = int(metrics[record["path"]]["churn_lines_window"])
        metrics[record["path"]]["age_days"] = age_days
        metrics[record["path"]]["relative_churn_window"] = round(
            churn_lines / max(record["lines"], 1), 6
        )

    return {path: dict(values) for path, values in metrics.items()} | {
        path: values for path, values in empty_metrics.items() if path not in metrics
    }
