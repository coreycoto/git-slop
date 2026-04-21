from __future__ import annotations

import contextlib
import io
import unittest
from pathlib import Path

from agent_tools.skills.metadata import (
    build_expected_outputs,
    load_and_validate_skill_metadata_manifest,
)

from git_slop.agent_skill_runtime import run_skill_entrypoint
from git_slop.agent_skills import ACTION_SPECS, SKILL_SPECS

REPO_ROOT = Path(__file__).resolve().parents[3]


class SkillRuntimeAndMetadataTests(unittest.TestCase):
    def test_manifest_covers_all_repo_local_skills(self) -> None:
        manifest = load_and_validate_skill_metadata_manifest(
            REPO_ROOT / "config" / "agents" / "skill_metadata_manifest.json",
            repo_root=REPO_ROOT,
            skills_root=REPO_ROOT / ".agents" / "skills",
        )
        self.assertEqual(
            sorted(manifest["skills"]),
            [
                "ensure-quarter-milestones",
                "github-backlog-mutate",
                "intake",
                "intake-preview",
                "label-palette-design",
                "plan-quarter-apply",
                "plan-quarter-preview",
                "review-to-backlog-apply",
                "review-to-backlog-preview",
            ],
        )
        expected_outputs = build_expected_outputs(manifest)
        self.assertIn(
            REPO_ROOT / ".agents" / "skills" / "intake" / "agents" / "openai.yaml",
            expected_outputs,
        )

    def test_skill_manifest_exposes_expected_actions(self) -> None:
        preview_spec = SKILL_SPECS["plan-quarter-preview"]
        self.assertIn("build-quarter-delta", preview_spec.actions)
        action_spec = ACTION_SPECS["label-palette"]
        self.assertEqual(action_spec.command, ("github", "sync-label-palette"))

    def test_skill_runtime_can_print_delegated_command(self) -> None:
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            exit_code = run_skill_entrypoint(
                skill_name="intake-preview",
                argv=[
                    "--repo-root",
                    str(REPO_ROOT),
                    "--print-command",
                    "digest",
                    "docs/vision.md",
                ],
                script_path=REPO_ROOT
                / ".agents"
                / "skills"
                / "intake-preview"
                / "scripts"
                / "run.py",
            )
        self.assertEqual(exit_code, 0)
        self.assertIn("agent_tools.cli", output.getvalue())
        self.assertIn("research digest", output.getvalue())
