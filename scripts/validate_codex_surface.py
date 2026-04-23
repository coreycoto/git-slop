from __future__ import annotations

import argparse
from pathlib import Path

from git_slop.integrations.agents.codex_surface import validate_codex_surface


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate the repo-local Codex runtime surface.")
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root to validate.",
    )
    parser.add_argument(
        "--require-codex-cli",
        action="store_true",
        help="Fail when the codex CLI is unavailable for execpolicy checks.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path(args.repo_root).resolve()
    errors = validate_codex_surface(repo_root, require_codex_cli=args.require_codex_cli)
    if errors:
        for error in errors:
            print(error)
        return 1
    print("Codex surface validation passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
