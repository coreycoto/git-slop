#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
import tomllib
from collections.abc import Callable
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[1]

Runner = Callable[[list[str], Path], None]


def project_version(project_root: Path) -> str:
    payload = tomllib.loads((project_root / "pyproject.toml").read_text(encoding="utf-8"))
    return str(payload["project"]["version"])


def validate_project_version(project_root: Path, expected_version: str) -> None:
    actual_version = project_version(project_root)
    if actual_version != expected_version:
        raise ValueError(
            f"pyproject.toml version is {actual_version}; expected {expected_version}."
        )


def tag_revision(project_root: Path, tag: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-list", "-n", "1", tag],
            cwd=project_root,
            stderr=subprocess.STDOUT,
            text=True,
        ).strip()
    except subprocess.CalledProcessError as error:
        output = error.output.strip()
        detail = f": {output}" if output else ""
        raise ValueError(f"release tag {tag} does not exist{detail}") from error


def run_command(command: list[str], cwd: Path) -> None:
    subprocess.run(command, cwd=cwd, check=True)


def prepare_release(
    *,
    version: str,
    tap: Path,
    project_root: Path = PROJECT_ROOT,
    runner: Runner = run_command,
) -> list[str]:
    tag = f"v{version}"
    validate_project_version(project_root, version)
    revision = tag_revision(project_root, tag)

    formula_path = (project_root / tap / "Formula" / "git-slop.rb").resolve()
    runner(["uv", "build"], project_root)
    runner(
        [
            "uv",
            "run",
            "python",
            "scripts/build_release_manifest.py",
            "--dist-dir",
            "dist",
            "--output",
            ".artifacts/releases/release-manifest.json",
            "--tag",
            tag,
        ],
        project_root,
    )
    runner(
        [
            "uv",
            "run",
            "python",
            "scripts/update_homebrew_formula.py",
            "--manifest",
            ".artifacts/releases/release-manifest.json",
            "--formula",
            str(formula_path),
        ],
        project_root,
    )

    return [
        f"Verified local tag {tag} at {revision}.",
        f"Push release tag: git push origin {tag}",
        "Watch release workflow: "
        "gh run list --repo coreycoto/git-slop --workflow release-publish.yml --limit 1",
        "Verify GitHub Release: gh release view "
        f"{tag} --repo coreycoto/git-slop --json url,tagName",
        f"Verify tap formula: cd {formula_path.parents[1]} && brew style Formula/git-slop.rb",
        "Fetch formula: brew fetch --force coreycoto/tap/git-slop",
        "Install formula: brew reinstall coreycoto/tap/git-slop",
        "Test formula: brew test coreycoto/tap/git-slop",
        "Confirm CLI: git-slop version",
        "Confirm Git manpage: git slop --help",
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description="Prepare a git-slop release locally.")
    parser.add_argument("--version", required=True, help="Release version, without leading v.")
    parser.add_argument(
        "--tap",
        default="../homebrew-tap",
        type=Path,
        help="Path to the checked-out coreycoto/homebrew-tap repository.",
    )
    args = parser.parse_args()

    for message in prepare_release(version=args.version, tap=args.tap):
        print(message)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
