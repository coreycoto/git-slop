from __future__ import annotations

from copy import deepcopy
from pathlib import Path
from typing import Any

import yaml

DEFAULT_CONFIG: dict[str, Any] = {
    "schema_version": 1,
    "inventory": {
        "ignore_globs": [
            "uv.lock",
            "poetry.lock",
            "Pipfile.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "bun.lock",
            "bun.lockb",
            "Cargo.lock",
            "Gemfile.lock",
            "composer.lock",
            "Podfile.lock",
        ]
    },
    "tokenizer": {
        "name": "cl100k_base",
    },
    "context_bands": {
        "compact_max_tokens": 3072,
        "healthy_max_tokens": 8000,
        "warning_max_tokens": 10000,
    },
    "history": {
        "churn_window_days": 180,
        "age_half_life_days": 180,
        "follow_renames": False,
    },
    "scoring": {
        "context_weight": 0.60,
        "age_weight": 0.20,
        "churn_weight": 0.20,
    },
    "check": {
        "fail_on_context_band": "critical",
        "fail_on_priority_band": "must_refactor",
    },
}

DEFAULT_SLOP_GITIGNORE = "/latest/\n/runs/\n/cache/\n"


def slop_dir(repo_root: Path) -> Path:
    return repo_root / ".slop"


def config_path(repo_root: Path) -> Path:
    return slop_dir(repo_root) / "config.yaml"


def slop_gitignore_path(repo_root: Path) -> Path:
    return slop_dir(repo_root) / ".gitignore"


def latest_dir(repo_root: Path) -> Path:
    return slop_dir(repo_root) / "latest"


def runs_dir(repo_root: Path) -> Path:
    return slop_dir(repo_root) / "runs"


def cache_dir(repo_root: Path) -> Path:
    return slop_dir(repo_root) / "cache"


def default_config() -> dict[str, Any]:
    return deepcopy(DEFAULT_CONFIG)


def _merge_nested(defaults: dict[str, Any], overrides: dict[str, Any]) -> dict[str, Any]:
    merged = deepcopy(defaults)
    for key, value in overrides.items():
        if key not in merged:
            merged[key] = value
            continue
        if isinstance(merged[key], dict) and isinstance(value, dict):
            merged[key] = _merge_nested(merged[key], value)
        else:
            merged[key] = value
    return merged


def normalize_config_payload(payload: dict[str, Any] | None) -> dict[str, Any]:
    if payload is None:
        return default_config()
    if not isinstance(payload, dict):
        raise ValueError("config.yaml must decode to a mapping.")
    merged = _merge_nested(DEFAULT_CONFIG, payload)
    if merged.get("schema_version") != 1:
        raise ValueError("config.yaml must declare schema_version: 1.")
    return merged


def load_config(repo_root: Path) -> dict[str, Any]:
    path = config_path(repo_root)
    if not path.exists():
        return default_config()
    payload = yaml.safe_load(path.read_text(encoding="utf-8"))
    return normalize_config_payload(payload)


def ensure_state_dirs(repo_root: Path) -> None:
    for path in (
        slop_dir(repo_root),
        latest_dir(repo_root),
        runs_dir(repo_root),
        cache_dir(repo_root),
    ):
        path.mkdir(parents=True, exist_ok=True)


def write_default_files(repo_root: Path, *, force: bool) -> dict[str, str]:
    ensure_state_dirs(repo_root)
    results: dict[str, str] = {}

    config_target = config_path(repo_root)
    if force or not config_target.exists():
        config_target.write_text(
            yaml.safe_dump(default_config(), sort_keys=False),
            encoding="utf-8",
        )
        results["config"] = "written"
    else:
        results["config"] = "kept"

    gitignore_target = slop_gitignore_path(repo_root)
    if force or not gitignore_target.exists():
        gitignore_target.write_text(DEFAULT_SLOP_GITIGNORE, encoding="utf-8")
        results["gitignore"] = "written"
    else:
        results["gitignore"] = "kept"
    return results
