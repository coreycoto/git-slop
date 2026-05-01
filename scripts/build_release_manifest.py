#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import tomllib
from pathlib import Path
from typing import Any

REPO_FULL_NAME = "coreycoto/git-slop"
PROJECT_ROOT = Path(__file__).resolve().parents[1]


def _project_version(project_root: Path) -> str:
    payload = tomllib.loads((project_root / "pyproject.toml").read_text(encoding="utf-8"))
    return str(payload["project"]["version"])


def _git_revision(project_root: Path, release_tag: str) -> str:
    return subprocess.check_output(
        ["git", "rev-list", "-n", "1", release_tag],
        cwd=project_root,
        text=True,
    ).strip()


def build_manifest(*, project_root: Path, dist_dir: Path, tag: str | None = None) -> dict[str, Any]:
    version = _project_version(project_root)
    release_tag = tag or f"v{version}"
    if not dist_dir.is_dir():
        raise ValueError(f"dist dir does not exist: {dist_dir}")
    return {
        "schema_version": 1,
        "project": "git-slop",
        "version": version,
        "tag": release_tag,
        "repository": REPO_FULL_NAME,
        "homebrew_source": {
            "url": f"https://github.com/{REPO_FULL_NAME}.git",
            "tag": release_tag,
            "revision": _git_revision(project_root, release_tag),
        },
        "install": {
            "homebrew_tap": [
                "brew tap coreycoto/tap",
                "brew install coreycoto/tap/git-slop",
            ],
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a git-slop Homebrew source manifest.")
    parser.add_argument("--dist-dir", default="dist")
    parser.add_argument("--output", default=".artifacts/releases/release-manifest.json")
    parser.add_argument("--tag")
    args = parser.parse_args()

    dist_dir = (PROJECT_ROOT / args.dist_dir).resolve()
    if not dist_dir.is_dir():
        raise SystemExit(f"dist dir does not exist: {dist_dir}")
    output = (PROJECT_ROOT / args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    manifest = build_manifest(project_root=PROJECT_ROOT, dist_dir=dist_dir, tag=args.tag)
    output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
