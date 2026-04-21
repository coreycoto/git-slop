from __future__ import annotations

from collections.abc import Sequence
from pathlib import Path

from agent_tools.skills.runtime import run_skill_entrypoint as run_external_skill_entrypoint

from .agent_skills import ACTION_SPECS, SKILL_SPECS


def run_skill_entrypoint(
    *,
    skill_name: str,
    argv: Sequence[str] | None = None,
    script_path: str | Path | None = None,
) -> int:
    return run_external_skill_entrypoint(
        skill_name=skill_name,
        skill_specs=SKILL_SPECS,
        action_specs=ACTION_SPECS,
        argv=argv,
        script_path=script_path,
    )
