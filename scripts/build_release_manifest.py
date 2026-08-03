#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tomllib
from collections.abc import Iterable
from pathlib import Path
from typing import Any

REPO_FULL_NAME = "coreycoto/git-slop"
PROJECT_ROOT = Path(__file__).resolve().parents[1]
SEMVER_TAG = re.compile(r"^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")

TARGETS: dict[str, dict[str, str]] = {
    "x86_64-unknown-linux-gnu": {
        "os": "linux",
        "arch": "x86_64",
        "archive": "tar.gz",
    },
    "aarch64-unknown-linux-gnu": {
        "os": "linux",
        "arch": "aarch64",
        "archive": "tar.gz",
    },
    "aarch64-apple-darwin": {
        "os": "macos",
        "arch": "aarch64",
        "archive": "tar.gz",
    },
    "x86_64-pc-windows-msvc": {
        "os": "windows",
        "arch": "x86_64",
        "archive": "zip",
    },
    "aarch64-pc-windows-msvc": {
        "os": "windows",
        "arch": "aarch64",
        "archive": "zip",
    },
}


def _project_version(project_root: Path) -> str:
    payload = tomllib.loads((project_root / "Cargo.toml").read_text(encoding="utf-8"))
    return str(payload["package"]["version"])


def _tag_version(tag: str) -> str:
    match = SEMVER_TAG.fullmatch(tag)
    if match is None:
        raise ValueError(f"release tag must be strict semver in vX.Y.Z form: {tag}")
    return tag.removeprefix("v")


def _git_revision(project_root: Path, release_tag: str) -> str:
    return subprocess.check_output(
        [
            "git",
            "rev-parse",
            "--verify",
            f"refs/tags/{release_tag}^{{commit}}",
        ],
        cwd=project_root,
        text=True,
    ).strip()


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _artifact_name(tag: str, target: str) -> str:
    archive = TARGETS[target]["archive"]
    return f"git-slop-{tag}-{target}.{archive}"


def _release_artifacts(
    *,
    dist_dir: Path,
    tag: str,
) -> list[dict[str, Any]]:
    release_url = f"https://github.com/{REPO_FULL_NAME}/releases/download/{tag}"

    artifacts: list[dict[str, Any]] = []
    missing: list[str] = []
    for target in sorted(TARGETS):
        name = _artifact_name(tag, target)
        path = dist_dir / name
        if not path.is_file():
            missing.append(name)
            continue
        metadata = TARGETS[target]
        artifacts.append(
            {
                "name": name,
                "path": name,
                "target": target,
                "os": metadata["os"],
                "arch": metadata["arch"],
                "archive": metadata["archive"],
                "sha256": _sha256(path),
                "size_bytes": path.stat().st_size,
                "url": f"{release_url}/{name}",
            }
        )

    if missing:
        raise ValueError(f"missing required release artifact(s): {', '.join(missing)}")

    expected_names = {artifact["name"] for artifact in artifacts}
    unexpected = sorted(
        path.name
        for path in dist_dir.iterdir()
        if path.is_file()
        and path.name.startswith(f"git-slop-{tag}-")
        and path.name not in expected_names
    )
    if unexpected:
        raise ValueError(f"unexpected release artifact(s): {', '.join(unexpected)}")

    return artifacts


def checksum_lines(artifacts: Iterable[dict[str, Any]]) -> str:
    return "".join(sorted(f"{artifact['sha256']}  {artifact['name']}\n" for artifact in artifacts))


def build_manifest(
    *,
    project_root: Path,
    dist_dir: Path,
    tag: str | None = None,
) -> dict[str, Any]:
    version = _project_version(project_root)
    release_tag = tag or f"v{version}"
    tag_version = _tag_version(release_tag)
    if tag_version != version:
        raise ValueError(
            f"Cargo.toml version is {version}; release tag {release_tag} is {tag_version}."
        )
    if not dist_dir.is_dir():
        raise ValueError(f"dist dir does not exist: {dist_dir}")

    artifacts = _release_artifacts(
        dist_dir=dist_dir,
        tag=release_tag,
    )
    release_url = f"https://github.com/{REPO_FULL_NAME}/releases/download/{release_tag}"
    revision = _git_revision(project_root, release_tag)
    return {
        "schema_version": 2,
        "project": "git-slop",
        "version": version,
        "tag": release_tag,
        "revision": revision,
        "repository": REPO_FULL_NAME,
        "artifacts": artifacts,
        "checksums": {
            "algorithm": "sha256",
            "name": "SHA256SUMS",
            "url": f"{release_url}/SHA256SUMS",
        },
        "homebrew_source": {
            "url": f"https://github.com/{REPO_FULL_NAME}.git",
            "tag": release_tag,
            "revision": revision,
        },
        "install": {
            "homebrew_tap": [
                "brew tap coreycoto/tap",
                "brew install coreycoto/tap/git-slop",
            ],
            "github_release": [
                (
                    f"gh release download {release_tag} --repo {REPO_FULL_NAME} "
                    f"--pattern 'git-slop-{release_tag}-<target>.*' "
                    "--pattern SHA256SUMS"
                ),
                "sha256sum --check SHA256SUMS --ignore-missing",
            ],
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build the cross-platform git-slop release manifest and checksums."
    )
    parser.add_argument("--dist-dir", default="dist")
    parser.add_argument(
        "--output",
        default="dist/release-manifest.json",
        help="Manifest path relative to the project root.",
    )
    parser.add_argument(
        "--checksum-output",
        default="dist/SHA256SUMS",
        help="Checksum path relative to the project root.",
    )
    parser.add_argument("--tag")
    args = parser.parse_args()

    dist_dir = (PROJECT_ROOT / args.dist_dir).resolve()
    output = (PROJECT_ROOT / args.output).resolve()
    checksum_output = (PROJECT_ROOT / args.checksum_output).resolve()
    manifest = build_manifest(
        project_root=PROJECT_ROOT,
        dist_dir=dist_dir,
        tag=args.tag,
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    checksum_output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    checksum_output.write_text(
        checksum_lines(manifest["artifacts"]),
        encoding="utf-8",
    )
    print(output)
    print(checksum_output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
