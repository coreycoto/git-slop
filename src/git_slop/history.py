from __future__ import annotations

import hashlib
import json
import math
from datetime import datetime, timedelta, timezone
from pathlib import Path, PurePosixPath
from typing import Any

from .config import cache_dir
from .git import has_head_commit, run_git

HISTORY_ANALYSIS_VERSION = 1


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
    parts = PurePosixPath(path).parts
    return parts[0] if parts else "."


def _percentile(values: list[float], quantile: float) -> float:
    if not values:
        return 0.0
    sorted_values = sorted(values)
    index = max(0, math.ceil(len(sorted_values) * quantile) - 1)
    return float(sorted_values[index])


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


def _json_fingerprint(payload: Any) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _inventory_fingerprint(records: list[dict[str, Any]]) -> str:
    payload = [record["path"] for record in sorted(records, key=lambda item: item["path"])]
    return _json_fingerprint(payload)


def _history_cache_key(
    repo_root: Path,
    records: list[dict[str, Any]],
    config: dict[str, Any],
) -> str:
    head_completed = run_git(repo_root, ["rev-parse", "--verify", "HEAD"])
    head_value = head_completed.stdout.strip() if head_completed.returncode == 0 else ""
    payload = {
        "analysis_version": HISTORY_ANALYSIS_VERSION,
        "head": head_value,
        "config": config,
        "inventory": _inventory_fingerprint(records),
    }
    return _json_fingerprint(payload)


def _cache_root(repo_root: Path, cache_key: str) -> Path:
    return cache_dir(repo_root) / "history" / cache_key


def _load_cached_json(path: Path) -> Any | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None


def _write_cached_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def _normalize_path(raw_path: str) -> str:
    return Path(raw_path).as_posix()


def _parse_status_log(raw_output: str) -> list[dict[str, Any]]:
    tokens = raw_output.split("\0")
    commits: list[dict[str, Any]] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if not token:
            index += 1
            continue
        if token != "commit" or index + 2 >= len(tokens):
            index += 1
            continue
        commit_sha = tokens[index + 1].strip()
        timestamp = _parse_unix_timestamp(tokens[index + 2]) or 0
        index += 3
        changes: list[dict[str, Any]] = []
        while index < len(tokens):
            token = tokens[index]
            if token == "commit":
                break
            index += 1
            if not token:
                continue
            status = token.lstrip("\n")
            if not status:
                continue
            kind_code = status[0]
            if kind_code in {"R", "C"}:
                if index + 1 >= len(tokens):
                    break
                old_path = tokens[index]
                new_path = tokens[index + 1]
                index += 2
                if not old_path or not new_path:
                    continue
                changes.append(
                    {
                        "status": status,
                        "kind": "rename" if kind_code == "R" else "copy",
                        "old_path": _normalize_path(old_path),
                        "new_path": _normalize_path(new_path),
                    }
                )
                continue
            if index >= len(tokens):
                break
            raw_path = tokens[index]
            index += 1
            if not raw_path:
                continue
            changes.append(
                {
                    "status": status,
                    "kind": "path",
                    "path": _normalize_path(raw_path),
                }
            )
        commits.append(
            {
                "commit": commit_sha,
                "timestamp": timestamp,
                "changes": changes,
            }
        )
    return commits


def _parse_numstat_log(raw_output: str) -> list[dict[str, Any]]:
    tokens = raw_output.split("\0")
    commits: list[dict[str, Any]] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if not token:
            index += 1
            continue
        if token != "commit" or index + 2 >= len(tokens):
            index += 1
            continue
        commit_sha = tokens[index + 1].strip()
        timestamp = _parse_unix_timestamp(tokens[index + 2]) or 0
        index += 3
        entries: list[dict[str, Any]] = []
        while index < len(tokens):
            token = tokens[index]
            if token == "commit":
                break
            index += 1
            if not token:
                continue
            stat_line = token.lstrip("\n")
            if not stat_line:
                continue
            parts = stat_line.split("\t")
            if len(parts) < 2:
                continue
            added_raw = parts[0]
            deleted_raw = parts[1]
            trailing_path = parts[2] if len(parts) > 2 else ""
            paths: list[str] = []
            if trailing_path:
                paths.append(_normalize_path(trailing_path))
            else:
                if index >= len(tokens) or tokens[index] == "commit":
                    break
                first_path = tokens[index]
                index += 1
                if first_path:
                    paths.append(_normalize_path(first_path))
                if index < len(tokens) and tokens[index] != "commit" and tokens[index]:
                    second_path = tokens[index]
                    index += 1
                    paths.append(_normalize_path(second_path))
            if added_raw == "-" or deleted_raw == "-":
                continue
            entries.append(
                {
                    "added": int(added_raw),
                    "deleted": int(deleted_raw),
                    "paths": paths,
                }
            )
        commits.append(
            {
                "commit": commit_sha,
                "timestamp": timestamp,
                "entries": entries,
            }
        )
    return commits


def _load_status_commits(
    repo_root: Path,
    *,
    since_utc: str | None,
    follow_renames: bool,
) -> list[dict[str, Any]]:
    args = [
        "log",
        "--name-status",
        "-z",
        "--format=commit%x00%H%x00%ct",
    ]
    if follow_renames:
        args.append("--find-renames")
    else:
        args.append("--no-renames")
    if since_utc is not None:
        args.append(f"--since={since_utc}")
    completed = run_git(repo_root, args)
    if completed.returncode != 0:
        return []
    return _parse_status_log(completed.stdout)


def _load_numstat_commits(
    repo_root: Path,
    *,
    since_utc: str,
    follow_renames: bool,
) -> list[dict[str, Any]]:
    args = [
        "log",
        "--numstat",
        "-z",
        "--format=commit%x00%H%x00%ct",
        f"--since={since_utc}",
    ]
    if follow_renames:
        args.append("--find-renames")
    else:
        args.append("--no-renames")
    completed = run_git(repo_root, args)
    if completed.returncode != 0:
        return []
    return _parse_numstat_log(completed.stdout)


def _token_density_map(records: list[dict[str, Any]]) -> dict[str, float]:
    density: dict[str, float] = {}
    for record in records:
        line_count = max(int(record["lines"]), 1)
        density[record["path"]] = max(float(record["tokens"]) / line_count, 1.0)
    return density


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


def _apply_rename_aliases(
    alias_to_current: dict[str, str],
    status_commit: dict[str, Any] | None,
) -> None:
    if status_commit is None:
        return
    for change in status_commit["changes"]:
        if change["kind"] != "rename":
            continue
        current_path = alias_to_current.get(change["new_path"])
        if current_path is not None:
            alias_to_current[change["old_path"]] = current_path


def _build_first_seen_exact(
    tracked_paths: set[str],
    status_commits: list[dict[str, Any]],
) -> dict[str, int | None]:
    appearance_timestamps: dict[str, int] = {}
    fallback_timestamps: dict[str, int] = {}
    for commit in status_commits:
        timestamp = int(commit["timestamp"])
        for change in commit["changes"]:
            if change["kind"] == "rename":
                new_path = change["new_path"]
                if new_path in tracked_paths:
                    appearance_timestamps[new_path] = timestamp
                    fallback_timestamps[new_path] = timestamp
                continue
            if change["kind"] != "path":
                continue
            path = change["path"]
            if path not in tracked_paths:
                continue
            fallback_timestamps[path] = timestamp
            if change["status"].startswith("A"):
                appearance_timestamps[path] = timestamp
    return {
        path: appearance_timestamps.get(path, fallback_timestamps.get(path))
        for path in sorted(tracked_paths)
    }


def _mapped_current_paths_for_change(
    change: dict[str, Any],
    alias_to_current: dict[str, str],
) -> set[str]:
    if change["kind"] == "rename":
        new_path = change["new_path"]
        old_path = change["old_path"]
        if new_path in alias_to_current:
            return {alias_to_current[new_path]}
        if old_path in alias_to_current:
            return {alias_to_current[old_path]}
        return set()
    if change["kind"] == "copy":
        new_path = change["new_path"]
        return {alias_to_current[new_path]} if new_path in alias_to_current else set()
    path = change["path"]
    return {alias_to_current[path]} if path in alias_to_current else set()


def _build_first_seen_with_lineage(
    tracked_paths: set[str],
    status_commits: list[dict[str, Any]],
) -> dict[str, int | None]:
    alias_to_current = {path: path for path in tracked_paths}
    earliest_timestamps: dict[str, int | None] = {
        path: None for path in sorted(tracked_paths)
    }
    for commit in status_commits:
        timestamp = int(commit["timestamp"])
        touched_paths: set[str] = set()
        for change in commit["changes"]:
            touched_paths.update(_mapped_current_paths_for_change(change, alias_to_current))
        for current_path in touched_paths:
            earliest_timestamps[current_path] = timestamp
        _apply_rename_aliases(alias_to_current, commit)
    return earliest_timestamps


def _map_numstat_entry_exact(
    entry: dict[str, Any],
    tracked_paths: set[str],
) -> str | None:
    if len(entry["paths"]) == 2:
        new_path = entry["paths"][1]
        old_path = entry["paths"][0]
        if new_path in tracked_paths:
            return new_path
        if old_path in tracked_paths:
            return old_path
        return None
    if not entry["paths"]:
        return None
    path = entry["paths"][0]
    return path if path in tracked_paths else None


def _map_numstat_entry_with_lineage(
    entry: dict[str, Any],
    alias_to_current: dict[str, str],
) -> str | None:
    if len(entry["paths"]) == 2:
        new_path = entry["paths"][1]
        old_path = entry["paths"][0]
        if new_path in alias_to_current:
            return alias_to_current[new_path]
        if old_path in alias_to_current:
            return alias_to_current[old_path]
        return None
    if not entry["paths"]:
        return None
    return alias_to_current.get(entry["paths"][0])


def _build_window_history_payload(
    *,
    tracked_paths: set[str],
    status_commits: list[dict[str, Any]],
    numstat_commits: list[dict[str, Any]],
    token_density: dict[str, float],
    follow_renames: bool,
) -> dict[str, Any]:
    metrics: dict[str, dict[str, float | int]] = {
        path: _empty_record_metrics() for path in sorted(tracked_paths)
    }
    commit_records: list[dict[str, Any]] = []
    status_by_commit = {commit["commit"]: commit for commit in status_commits}
    alias_to_current = {path: path for path in tracked_paths}

    for commit in numstat_commits:
        per_commit_entries: dict[str, dict[str, Any]] = {}
        seen_revision_paths: set[str] = set()
        for entry in commit["entries"]:
            if follow_renames:
                current_path = _map_numstat_entry_with_lineage(entry, alias_to_current)
            else:
                current_path = _map_numstat_entry_exact(entry, tracked_paths)
            if current_path is None:
                continue

            added = int(entry["added"])
            deleted = int(entry["deleted"])
            line_delta = added + deleted

            record_metrics = metrics[current_path]
            if current_path not in seen_revision_paths:
                record_metrics["revisions_window"] += 1
                seen_revision_paths.add(current_path)
            record_metrics["added_window"] += added
            record_metrics["deleted_window"] += deleted
            record_metrics["churn_lines_window"] += line_delta

            commit_entry = per_commit_entries.setdefault(
                current_path,
                {
                    "path": current_path,
                    "added": 0,
                    "deleted": 0,
                    "line_delta": 0,
                    "token_delta": 0,
                    "top_level_root": _top_level_root(current_path),
                },
            )
            commit_entry["added"] += added
            commit_entry["deleted"] += deleted
            commit_entry["line_delta"] += line_delta
            commit_entry["token_delta"] += int(
                round(line_delta * token_density.get(current_path, 1.0))
            )

        if per_commit_entries:
            file_entries = sorted(per_commit_entries.values(), key=lambda item: item["path"])
            line_deltas = [int(item["line_delta"]) for item in file_entries]
            roots = sorted({item["top_level_root"] for item in file_entries})
            commit_records.append(
                {
                    "commit": commit["commit"],
                    "timestamp": int(commit["timestamp"]),
                    "file_count": len(file_entries),
                    "top_level_root_count": len(roots),
                    "top_level_roots": roots,
                    "total_line_delta": sum(line_deltas),
                    "total_token_delta": sum(int(item["token_delta"]) for item in file_entries),
                    "change_entropy": round(_shannon_entropy(line_deltas), 6),
                    "files": file_entries,
                }
            )

        if follow_renames:
            _apply_rename_aliases(alias_to_current, status_by_commit.get(commit["commit"]))

    return {
        "file_metrics": metrics,
        "commit_records": commit_records,
        "repo_baselines": _build_repo_baselines(commit_records),
    }


def _build_history_snapshot_uncached(
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
    token_density = _token_density_map(records)

    full_status_commits = _load_status_commits(
        repo_root,
        since_utc=None,
        follow_renames=follow_renames,
    )
    if follow_renames:
        first_seen_timestamps = _build_first_seen_with_lineage(tracked_paths, full_status_commits)
    else:
        first_seen_timestamps = _build_first_seen_exact(tracked_paths, full_status_commits)

    window_status_commits = _load_status_commits(
        repo_root,
        since_utc=since_utc,
        follow_renames=follow_renames,
    )
    window_numstat_commits = _load_numstat_commits(
        repo_root,
        since_utc=since_utc,
        follow_renames=follow_renames,
    )
    window_payload = _build_window_history_payload(
        tracked_paths=tracked_paths,
        status_commits=window_status_commits,
        numstat_commits=window_numstat_commits,
        token_density=token_density,
        follow_renames=follow_renames,
    )

    for record in records:
        record_metrics = window_payload["file_metrics"].get(record["path"], _empty_record_metrics())
        first_seen_timestamp = first_seen_timestamps.get(record["path"])
        age_days = _age_days_from_timestamp(first_seen_timestamp, now=now)
        churn_lines = int(record_metrics["churn_lines_window"])
        record_metrics["age_days"] = age_days
        record_metrics["relative_churn_window"] = round(
            churn_lines / max(record["lines"], 1),
            6,
        )
        window_payload["file_metrics"][record["path"]] = record_metrics

    return window_payload


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

    cache_key = _history_cache_key(repo_root, records, config)
    cache_path = _cache_root(repo_root, cache_key) / "history_snapshot.json"
    cached_snapshot = _load_cached_json(cache_path)
    if cached_snapshot is not None:
        return cached_snapshot

    snapshot = _build_history_snapshot_uncached(repo_root, records, config)
    _write_cached_json(cache_path, snapshot)
    return snapshot


def build_history_metrics(
    repo_root: Path,
    records: list[dict[str, Any]],
    config: dict[str, Any],
) -> dict[str, dict[str, float | int]]:
    return build_history_snapshot(repo_root, records, config)["file_metrics"]
