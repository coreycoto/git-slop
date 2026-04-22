from __future__ import annotations

import math
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


def _empty_record_metrics() -> dict[str, float | int]:
    return {
        "age_days": 0,
        "revisions_window": 0,
        "added_window": 0,
        "deleted_window": 0,
        "churn_lines_window": 0,
        "relative_churn_window": 0.0,
    }


def _top_level_root(path: str) -> str:
    parts = Path(path).parts
    return parts[0] if parts else "."


def _percentile(values: list[float], quantile: float) -> float:
    if not values:
        return 0.0
    sorted_values = sorted(values)
    index = max(0, math.ceil(len(sorted_values) * quantile) - 1)
    return float(sorted_values[index])


def _parse_numstat_line(line: str) -> tuple[int, int] | None:
    parts = line.split("\t", 2)
    if len(parts) != 3:
        return None
    added_raw, deleted_raw, _path = parts
    if added_raw == "-" or deleted_raw == "-":
        return None
    return int(added_raw), int(deleted_raw)


def _shannon_entropy(weights: list[int]) -> float:
    total = sum(weights)
    if total <= 0:
        return 0.0
    entropy = 0.0
    for weight in weights:
        if weight <= 0:
            continue
        probability = weight / total
        entropy -= probability * math.log2(probability)
    return entropy


def _build_window_metrics_with_path_filter(
    repo_root: Path,
    tracked_paths: set[str],
    since_utc: str,
) -> dict[str, dict[str, float | int]]:
    metrics = defaultdict(_empty_record_metrics)
    completed = run_git(
        repo_root,
        ["log", "--numstat", "--format=commit:%H", f"--since={since_utc}", "--no-renames"],
    )
    if completed.returncode != 0:
        return {}
    for line in completed.stdout.splitlines():
        if not line or line.startswith("commit:"):
            continue
        parsed = _parse_numstat_line(line)
        if parsed is None:
            continue
        path = line.split("\t", 2)[2]
        normalized_path = Path(path).as_posix()
        if normalized_path not in tracked_paths:
            continue
        added, deleted = parsed
        metrics[normalized_path]["revisions_window"] += 1
        metrics[normalized_path]["added_window"] += added
        metrics[normalized_path]["deleted_window"] += deleted
        metrics[normalized_path]["churn_lines_window"] += added + deleted
    return {path: dict(values) for path, values in metrics.items()}


def _build_window_metrics_following_renames(
    repo_root: Path,
    record_path: str,
    since_utc: str,
) -> dict[str, float | int]:
    completed = run_git(
        repo_root,
        [
            "log",
            "--follow",
            "--numstat",
            "--format=commit:%H",
            f"--since={since_utc}",
            "--",
            record_path,
        ],
    )
    metrics = _empty_record_metrics()
    if completed.returncode != 0:
        return metrics

    current_commit: str | None = None
    commit_has_change = False
    for line in completed.stdout.splitlines():
        if line.startswith("commit:"):
            if current_commit is not None and commit_has_change:
                metrics["revisions_window"] += 1
            current_commit = line.removeprefix("commit:").strip() or None
            commit_has_change = False
            continue
        if not line:
            continue
        parsed = _parse_numstat_line(line)
        if parsed is None:
            continue
        added, deleted = parsed
        metrics["added_window"] += added
        metrics["deleted_window"] += deleted
        metrics["churn_lines_window"] += added + deleted
        commit_has_change = True

    if current_commit is not None and commit_has_change:
        metrics["revisions_window"] += 1
    return metrics


def _renamed_paths_in_window(
    repo_root: Path,
    tracked_paths: set[str],
    since_utc: str,
) -> set[str]:
    completed = run_git(
        repo_root,
        [
            "log",
            "--name-status",
            "--format=commit:%H",
            "--find-renames",
            "--diff-filter=R",
            f"--since={since_utc}",
        ],
    )
    if completed.returncode != 0:
        return set()

    renamed_paths: set[str] = set()
    for line in completed.stdout.splitlines():
        if not line or line.startswith("commit:"):
            continue
        parts = line.split("\t")
        if len(parts) < 3 or not parts[0].startswith("R"):
            continue
        normalized_new_path = Path(parts[2]).as_posix()
        if normalized_new_path in tracked_paths:
            renamed_paths.add(normalized_new_path)
    return renamed_paths


def _first_seen_timestamp_for_path(
    repo_root: Path,
    record_path: str,
    *,
    follow_renames: bool,
) -> int | None:
    if follow_renames:
        age_args = ["log", "--follow", "--format=%ct", "--", record_path]
    else:
        age_args = ["log", "--diff-filter=A", "--format=%ct", "--reverse", "--", record_path]
    completed = run_git(repo_root, age_args)
    if completed.returncode != 0:
        return None
    lines = [line for line in completed.stdout.splitlines() if line.strip()]
    if not lines:
        return None
    return _parse_unix_timestamp(lines[-1] if follow_renames else lines[0])


def _token_density_map(records: list[dict[str, Any]]) -> dict[str, float]:
    density: dict[str, float] = {}
    for record in records:
        line_count = max(int(record["lines"]), 1)
        density[record["path"]] = max(float(record["tokens"]) / line_count, 1.0)
    return density


def _build_commit_records(
    repo_root: Path,
    tracked_paths: set[str],
    since_utc: str,
    token_density: dict[str, float],
) -> list[dict[str, Any]]:
    completed = run_git(
        repo_root,
        ["log", "--numstat", "--format=commit:%H\t%ct", f"--since={since_utc}", "--no-renames"],
    )
    if completed.returncode != 0:
        return []

    commit_records: list[dict[str, Any]] = []
    current_commit: dict[str, Any] | None = None

    def flush_commit() -> None:
        nonlocal current_commit
        if current_commit is None or not current_commit["files"]:
            current_commit = None
            return
        file_entries = sorted(current_commit["files"], key=lambda item: item["path"])
        line_deltas = [int(item["line_delta"]) for item in file_entries]
        roots = sorted({item["top_level_root"] for item in file_entries})
        commit_records.append(
            {
                "commit": current_commit["commit"],
                "timestamp": current_commit["timestamp"],
                "file_count": len(file_entries),
                "top_level_root_count": len(roots),
                "top_level_roots": roots,
                "total_line_delta": sum(line_deltas),
                "total_token_delta": sum(int(item["token_delta"]) for item in file_entries),
                "change_entropy": round(_shannon_entropy(line_deltas), 6),
                "files": file_entries,
            }
        )
        current_commit = None

    for raw_line in completed.stdout.splitlines():
        if raw_line.startswith("commit:"):
            flush_commit()
            parts = raw_line.removeprefix("commit:").split("\t", 1)
            commit_sha = parts[0].strip()
            timestamp = _parse_unix_timestamp(parts[1]) if len(parts) > 1 else None
            current_commit = {
                "commit": commit_sha,
                "timestamp": timestamp or 0,
                "files": [],
            }
            continue
        if not raw_line or current_commit is None:
            continue
        parsed = _parse_numstat_line(raw_line)
        if parsed is None:
            continue
        path = Path(raw_line.split("\t", 2)[2]).as_posix()
        if path not in tracked_paths:
            continue
        added, deleted = parsed
        line_delta = added + deleted
        current_commit["files"].append(
            {
                "path": path,
                "added": added,
                "deleted": deleted,
                "line_delta": line_delta,
                "token_delta": int(round(line_delta * token_density.get(path, 1.0))),
                "top_level_root": _top_level_root(path),
            }
        )

    flush_commit()
    return commit_records


def _build_repo_baselines(commit_records: list[dict[str, Any]]) -> dict[str, float]:
    if not commit_records:
        return {
            "p95_files_touched": 0.0,
            "p99_files_touched": 0.0,
            "p95_token_delta_mass": 0.0,
            "p95_top_level_root_spread": 0.0,
            "p95_change_entropy": 0.0,
        }
    return {
        "p95_files_touched": _percentile(
            [float(record["file_count"]) for record in commit_records],
            0.95,
        ),
        "p99_files_touched": _percentile(
            [float(record["file_count"]) for record in commit_records],
            0.99,
        ),
        "p95_token_delta_mass": _percentile(
            [float(record["total_token_delta"]) for record in commit_records],
            0.95,
        ),
        "p95_top_level_root_spread": _percentile(
            [float(record["top_level_root_count"]) for record in commit_records],
            0.95,
        ),
        "p95_change_entropy": _percentile(
            [float(record["change_entropy"]) for record in commit_records],
            0.95,
        ),
    }


def build_history_snapshot(
    repo_root: Path,
    records: list[dict[str, Any]],
    config: dict[str, Any],
) -> dict[str, Any]:
    empty_metrics = {record["path"]: _empty_record_metrics() for record in records}
    empty_snapshot = {
        "file_metrics": empty_metrics,
        "commit_records": [],
        "repo_baselines": _build_repo_baselines([]),
    }
    if not records or not has_head_commit(repo_root):
        return empty_snapshot

    now = datetime.now(timezone.utc)
    history_config = config["history"]
    window_days = int(history_config["churn_window_days"])
    follow_renames = bool(history_config["follow_renames"])
    since_utc = (now - timedelta(days=window_days)).strftime("%Y-%m-%dT%H:%M:%SZ")
    tracked_paths = {record["path"] for record in records}
    metrics = _build_window_metrics_with_path_filter(repo_root, tracked_paths, since_utc)
    if follow_renames:
        for renamed_path in _renamed_paths_in_window(repo_root, tracked_paths, since_utc):
            metrics[renamed_path] = _build_window_metrics_following_renames(
                repo_root,
                renamed_path,
                since_utc,
            )

    for record in records:
        record_metrics = metrics.get(record["path"], _empty_record_metrics())
        first_seen_timestamp = _first_seen_timestamp_for_path(
            repo_root,
            record["path"],
            follow_renames=follow_renames,
        )
        age_days = _age_days_from_timestamp(first_seen_timestamp, now=now)
        churn_lines = int(record_metrics["churn_lines_window"])
        record_metrics["age_days"] = age_days
        record_metrics["relative_churn_window"] = round(
            churn_lines / max(record["lines"], 1), 6
        )
        metrics[record["path"]] = record_metrics

    file_metrics = {path: dict(values) for path, values in metrics.items()} | {
        path: values for path, values in empty_metrics.items() if path not in metrics
    }
    commit_records = _build_commit_records(
        repo_root,
        tracked_paths,
        since_utc,
        _token_density_map(records),
    )
    return {
        "file_metrics": file_metrics,
        "commit_records": commit_records,
        "repo_baselines": _build_repo_baselines(commit_records),
    }


def build_history_metrics(
    repo_root: Path,
    records: list[dict[str, Any]],
    config: dict[str, Any],
) -> dict[str, dict[str, float | int]]:
    return build_history_snapshot(repo_root, records, config)["file_metrics"]
