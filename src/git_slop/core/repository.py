from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Sequence


def run_git(
    repo_root: Path,
    args: Sequence[str],
    *,
    check: bool = False,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=check,
    )


def resolve_repo_root(start_path: str | Path | None = None) -> Path:
    cwd = (
        Path(start_path).expanduser().resolve()
        if start_path is not None
        else Path.cwd().resolve()
    )
    completed = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ValueError("git-slop must run inside a Git repository.")
    return Path(completed.stdout.strip()).resolve()


def list_tracked_files(repo_root: Path) -> list[str]:
    completed = run_git(repo_root, ["ls-files", "-z"], check=True)
    return [path for path in completed.stdout.split("\0") if path]


def has_head_commit(repo_root: Path) -> bool:
    completed = run_git(repo_root, ["rev-parse", "--verify", "HEAD"])
    return completed.returncode == 0


def repo_metadata(repo_root: Path) -> dict[str, str | bool | None]:
    branch = run_git(repo_root, ["branch", "--show-current"]).stdout.strip() or None
    head = run_git(repo_root, ["rev-parse", "--verify", "HEAD"])
    head_commit = head.stdout.strip() if head.returncode == 0 else None
    head_commit_timestamp = None
    if head_commit is not None:
        timestamp = run_git(repo_root, ["log", "-1", "--format=%cI", "HEAD"])
        head_commit_timestamp = timestamp.stdout.strip() or None
    remote = run_git(repo_root, ["config", "--get", "remote.origin.url"]).stdout.strip() or None
    return {
        "repo_root": str(repo_root),
        "repo_name": repo_root.name,
        "branch": branch,
        "head_commit": head_commit,
        "head_commit_timestamp": head_commit_timestamp,
        "git_remote_url": remote,
        "has_head_commit": head_commit is not None,
    }
