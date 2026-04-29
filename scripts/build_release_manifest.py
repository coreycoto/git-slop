#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
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


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _git_revision(project_root: Path, release_tag: str) -> str:
    return subprocess.check_output(
        ["git", "rev-list", "-n", "1", release_tag],
        cwd=project_root,
        text=True,
    ).strip()


def _artifact_path(path: Path, project_root: Path) -> str:
    try:
        return path.resolve().relative_to(project_root.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def build_manifest(*, project_root: Path, dist_dir: Path, tag: str | None = None) -> dict[str, Any]:
    version = _project_version(project_root)
    release_tag = tag or f"v{version}"
    artifacts = []
    for path in sorted(dist_dir.iterdir()):
        if path.suffix not in {".whl", ".gz"}:
            continue
        if version not in path.name:
            continue
        artifacts.append(
            {
                "name": path.name,
                "path": _artifact_path(path, project_root),
                "sha256": _sha256(path),
                "size_bytes": path.stat().st_size,
            }
        )
    wheel_names = [artifact["name"] for artifact in artifacts if artifact["name"].endswith(".whl")]
    wheel_name = wheel_names[0] if wheel_names else None
    release_url = f"https://github.com/{REPO_FULL_NAME}/releases/download/{release_tag}"
    return {
        "schema_version": 1,
        "project": "git-slop",
        "version": version,
        "tag": release_tag,
        "repository": REPO_FULL_NAME,
        "homebrew_source": {
            "url": f"ssh://git@github.com/{REPO_FULL_NAME}.git",
            "tag": release_tag,
            "revision": _git_revision(project_root, release_tag),
        },
        "artifacts": artifacts,
        "wheel": {
            "name": wheel_name,
            "url": f"{release_url}/{wheel_name}" if wheel_name else None,
            "sha256": next(
                (artifact["sha256"] for artifact in artifacts if artifact["name"] == wheel_name),
                None,
            ),
        },
        "install": {
            "uv_release_wheel": [
                (
                    f"gh release download {release_tag} --repo {REPO_FULL_NAME} "
                    "--pattern 'git_slop-*.whl' --dir .artifacts/git-slop"
                ),
                "shasum -a 256 .artifacts/git-slop/<wheel>",
                "uv tool install --force .artifacts/git-slop/<wheel>",
            ],
            "homebrew_private_tap": [
                "brew tap coreycoto/tap git@github.com:coreycoto/homebrew-tap.git",
                "brew install coreycoto/tap/git-slop",
            ],
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a git-slop release manifest.")
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
