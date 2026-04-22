from __future__ import annotations

from fnmatch import fnmatch
from pathlib import Path
from typing import Any

NULL_BYTE_WINDOW = 4096


def _is_binary(raw_bytes: bytes) -> bool:
    return b"\x00" in raw_bytes[:NULL_BYTE_WINDOW]


def _count_lines(text: str) -> int:
    if not text:
        return 0
    return text.count("\n") + (0 if text.endswith("\n") else 1)


def _is_ignored(relative_path: str, ignore_globs: list[str]) -> bool:
    path_name = Path(relative_path).name
    return any(
        fnmatch(relative_path, pattern) or fnmatch(path_name, pattern)
        for pattern in ignore_globs
    )


def build_inventory(
    repo_root: Path,
    tracked_paths: list[str],
    *,
    ignore_globs: list[str],
) -> tuple[list[dict[str, Any]], dict[str, int]]:
    records: list[dict[str, Any]] = []
    skipped = {
        "ignored": 0,
        "missing": 0,
        "binary": 0,
        "undecodable": 0,
    }
    for relative_path in tracked_paths:
        if _is_ignored(relative_path, ignore_globs):
            skipped["ignored"] += 1
            continue
        absolute_path = repo_root / relative_path
        if not absolute_path.exists():
            skipped["missing"] += 1
            continue
        raw_bytes = absolute_path.read_bytes()
        if _is_binary(raw_bytes):
            skipped["binary"] += 1
            continue
        try:
            text = raw_bytes.decode("utf-8")
        except UnicodeDecodeError:
            skipped["undecodable"] += 1
            continue
        records.append(
            {
                "path": Path(relative_path).as_posix(),
                "bytes": len(raw_bytes),
                "lines": _count_lines(text),
                "text": text,
            }
        )
    return records, skipped
