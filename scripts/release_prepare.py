#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import subprocess
import tomllib
from collections.abc import Callable
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[1]
SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")

Runner = Callable[[list[str], Path], None]


def project_version(project_root: Path) -> str:
    payload = tomllib.loads((project_root / "Cargo.toml").read_text(encoding="utf-8"))
    return str(payload["package"]["version"])


def validate_project_version(project_root: Path, expected_version: str) -> None:
    if SEMVER.fullmatch(expected_version) is None:
        raise ValueError(f"release version must be strict semver in X.Y.Z form: {expected_version}")
    actual_version = project_version(project_root)
    if actual_version != expected_version:
        raise ValueError(f"Cargo.toml version is {actual_version}; expected {expected_version}.")


def _git_output(project_root: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", *args],
            cwd=project_root,
            stderr=subprocess.STDOUT,
            text=True,
        ).strip()
    except subprocess.CalledProcessError as error:
        output = error.output.strip()
        detail = f": {output}" if output else ""
        raise ValueError(f"git {' '.join(args)} failed{detail}") from error


def tag_revision(project_root: Path, tag: str) -> str:
    try:
        return _git_output(
            project_root,
            "rev-parse",
            "--verify",
            f"refs/tags/{tag}^{{commit}}",
        )
    except ValueError as error:
        raise ValueError(f"release tag {tag} does not exist: {error}") from error


def head_revision(project_root: Path) -> str:
    return _git_output(project_root, "rev-parse", "HEAD")


def validate_release_state(*, project_root: Path, version: str) -> tuple[str, str]:
    validate_project_version(project_root, version)
    tag = f"v{version}"
    revision = tag_revision(project_root, tag)
    head = head_revision(project_root)
    if revision != head:
        raise ValueError(
            f"release tag {tag} resolves to {revision}, but HEAD is {head}; "
            "prepare and publish from the exact tagged commit."
        )
    return tag, revision


def run_command(command: list[str], cwd: Path) -> None:
    subprocess.run(command, cwd=cwd, check=True)


def prepare_release(
    *,
    version: str,
    tap: Path,
    project_root: Path = PROJECT_ROOT,
    runner: Runner = run_command,
) -> list[str]:
    tag, revision = validate_release_state(project_root=project_root, version=version)
    formula_path = (project_root / tap / "Formula" / "git-slop.rb").resolve()

    commands = [
        ["cargo", "fmt", "--all", "--", "--check"],
        [
            "cargo",
            "clippy",
            "--all-targets",
            "--all-features",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        ["cargo", "test", "--all-targets", "--all-features", "--locked"],
        ["cargo", "package", "--locked"],
        ["cargo", "publish", "--dry-run", "--locked"],
        [
            "python3",
            "scripts/update_homebrew_formula.py",
            "--tag",
            tag,
            "--version",
            version,
            "--revision",
            revision,
            "--formula",
            str(formula_path),
        ],
    ]
    for command in commands:
        runner(command, project_root)

    return [
        f"Verified local tag {tag} at {revision}.",
        "Validated formatting, linting, tests, Cargo packaging, and crates.io dry-run.",
        f"Prepared native Rust Homebrew formula: {formula_path}",
        f"Push release tag: git push origin {tag}",
        "Watch release workflow: "
        "gh run list --repo coreycoto/git-slop --workflow release-publish.yml --limit 1",
        "Verify GitHub Release assets: gh release view "
        f"{tag} --repo coreycoto/git-slop --json url,tagName,assets",
        f"Verify tap formula: cd {formula_path.parents[1]} && brew style Formula/git-slop.rb",
        "Upgrade lane (install the prior tap formula before merging the tap update): "
        "brew update && brew upgrade coreycoto/tap/git-slop",
        "Clean-install lane (use a separate clean runner): brew install coreycoto/tap/git-slop",
        "Test both lanes: brew test coreycoto/tap/git-slop",
        "Confirm CLI: git-slop version",
        "Confirm Git command: git slop --help",
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description="Prepare a git-slop Rust release locally.")
    parser.add_argument("--version", required=True, help="Release version, without leading v.")
    parser.add_argument(
        "--tap",
        default="../homebrew-tap",
        type=Path,
        help="Path to the checked-out coreycoto/homebrew-tap repository.",
    )
    parser.add_argument(
        "--check-only",
        action="store_true",
        help="Only validate that HEAD, the tag, and Cargo.toml agree.",
    )
    args = parser.parse_args()

    if args.check_only:
        tag, revision = validate_release_state(
            project_root=PROJECT_ROOT,
            version=args.version,
        )
        print(f"Verified local tag {tag} at {revision}.")
        return 0

    for message in prepare_release(version=args.version, tap=args.tap):
        print(message)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
