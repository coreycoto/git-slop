from __future__ import annotations

import contextlib
import io
import unittest
from pathlib import Path

from git_slop.agent_skill_runtime import run_skill_entrypoint
from git_slop.agent_skills import ACTION_SPECS, SKILL_SPECS
from git_slop.integrations.agents.codex_surface import PLUGIN_SKILL_CATALOG

REPO_ROOT = Path(__file__).resolve().parents[3]


class SkillRuntimeAndMetadataTests(unittest.TestCase):
    def test_plugin_metadata_covers_all_repo_runtime_skills(self) -> None:
        self.assertTrue(set(SKILL_SPECS).issubset(PLUGIN_SKILL_CATALOG))
        self.assertIn("intake", PLUGIN_SKILL_CATALOG)

    def test_skill_manifest_exposes_expected_actions(self) -> None:
        preview_spec = SKILL_SPECS["plan-quarter-preview"]
        self.assertIn("build-quarter-delta", preview_spec.actions)
        backlog_preview_spec = SKILL_SPECS["plan-to-backlog-preview"]
        self.assertIn("plan-to-backlog", backlog_preview_spec.actions)
        action_spec = ACTION_SPECS["label-palette"]
        self.assertEqual(
            action_spec.command,
            ("agent_plugins.github.governance.sync_label_palette",),
        )

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
                script_path=REPO_ROOT,
            )
        self.assertEqual(exit_code, 0)
        self.assertIn("agent_plugins.research.digest", output.getvalue())
        self.assertIn("docs/vision.md", output.getvalue())
