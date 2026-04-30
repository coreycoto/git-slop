from __future__ import annotations

import argparse
import shlex
import subprocess
import sys
from collections.abc import Mapping, Sequence
from pathlib import Path

from git_slop.core.repository import resolve_repo_root

from .skills import ACTION_SPECS, SKILL_SPECS, ActionSpec, SkillSpec

try:
    from agent_plugins.contracts.skills import (
        get_action_spec,
        get_action_spec_for_any_skill,
        get_skill_spec,
    )
except ModuleNotFoundError:

    def get_skill_spec(
        skill_specs: Mapping[str, SkillSpec],
        skill_name: str,
    ) -> SkillSpec:
        try:
            return skill_specs[skill_name]
        except KeyError as error:
            raise ValueError(f"Unknown skill: {skill_name}") from error

    def get_action_spec(
        skill_specs: Mapping[str, SkillSpec],
        action_specs: Mapping[str, ActionSpec],
        skill_name: str,
        action_name: str,
    ) -> ActionSpec:
        skill_spec = get_skill_spec(skill_specs, skill_name)
        if action_name not in skill_spec.actions:
            raise ValueError(f"Skill {skill_name} does not support action: {action_name}")
        try:
            return action_specs[action_name]
        except KeyError as error:
            raise ValueError(f"Unknown action: {action_name}") from error

    def get_action_spec_for_any_skill(
        action_specs: Mapping[str, ActionSpec],
        action_name: str,
    ) -> ActionSpec:
        try:
            return action_specs[action_name]
        except KeyError as error:
            raise ValueError(f"Unknown action: {action_name}") from error


def _render_supported_actions(
    skill_name: str,
    *,
    skill_specs: Mapping[str, SkillSpec],
    action_specs: Mapping[str, ActionSpec],
) -> str:
    skill_spec = get_skill_spec(skill_specs, skill_name)
    lines = ["Supported actions:"]
    for action_name in skill_spec.actions:
        action_spec = get_action_spec(skill_specs, action_specs, skill_name, action_name)
        lines.append(f"  {action_name:<24} {action_spec.description}")
    return "\n".join(lines)


def _build_parser(
    skill_name: str,
    *,
    skill_specs: Mapping[str, SkillSpec],
    action_specs: Mapping[str, ActionSpec],
) -> argparse.ArgumentParser:
    skill_spec = get_skill_spec(skill_specs, skill_name)
    parser = argparse.ArgumentParser(
        description=skill_spec.description,
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=_render_supported_actions(
            skill_name,
            skill_specs=skill_specs,
            action_specs=action_specs,
        ),
    )
    parser.add_argument("--repo-root", help="Repository root. Defaults to the current repo.")
    parser.add_argument(
        "--print-command",
        action="store_true",
        help="Print the delegated command instead of executing it.",
    )
    parser.add_argument("action", nargs="?", help="Deterministic action to run for this skill.")
    parser.add_argument(
        "action_args",
        nargs=argparse.REMAINDER,
        help="Arguments forwarded to the delegated runtime module.",
    )
    return parser


def _module_command(module_name: str) -> list[str]:
    return [
        sys.executable,
        "-c",
        f"from {module_name} import main; raise SystemExit(main())",
    ]


def _build_command(
    action_name: str,
    *,
    repo_root: Path,
    forwarded_args: Sequence[str],
    action_specs: Mapping[str, ActionSpec],
) -> list[str]:
    action_spec = get_action_spec_for_any_skill(action_specs, action_name)
    module_name, *module_args = action_spec.command
    return [
        *_module_command(module_name),
        *module_args,
        "--repo-root",
        str(repo_root),
        *forwarded_args,
    ]


def run_skill_entrypoint(
    *,
    skill_name: str,
    argv: Sequence[str] | None = None,
    script_path: str | Path | None = None,
) -> int:
    parser = _build_parser(skill_name, skill_specs=SKILL_SPECS, action_specs=ACTION_SPECS)
    args = parser.parse_args(list(argv) if argv is not None else None)
    if not args.action:
        parser.print_help()
        return 0

    repo_root = resolve_repo_root(args.repo_root or script_path)
    get_action_spec(SKILL_SPECS, ACTION_SPECS, skill_name, args.action)
    command = _build_command(
        args.action,
        repo_root=repo_root,
        forwarded_args=args.action_args,
        action_specs=ACTION_SPECS,
    )
    if args.print_command:
        print(shlex.join(command))
        return 0

    completed = subprocess.run(command, cwd=repo_root, check=False)
    return completed.returncode


__all__ = ["run_skill_entrypoint"]
