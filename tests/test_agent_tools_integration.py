from __future__ import annotations

import contextlib
import io
import subprocess
import sys
import unittest
from pathlib import Path

import yaml

from git_slop.agent_skill_runtime import run_skill_entrypoint
from git_slop.agent_skills import ACTION_SPECS, SKILL_SPECS
from git_slop.integrations.agents.codex_surface import PLUGIN_SKILL_CATALOG

REPO_ROOT = Path(__file__).resolve().parents[1]


class AgentToolsIntegrationTests(unittest.TestCase):
    def test_external_agent_tools_cli_is_available(self) -> None:
        completed = subprocess.run(
            [
                sys.executable,
                "-m",
                "agent_tools",
                "github",
                "project-snapshot",
                "--repo-root",
                str(REPO_ROOT),
            ],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("git-slop", completed.stdout)

    def test_plugin_skill_metadata_covers_repo_runtime_skills(self) -> None:
        self.assertTrue(set(SKILL_SPECS).issubset(PLUGIN_SKILL_CATALOG))
        for skill_name in SKILL_SPECS:
            metadata_path = (
                REPO_ROOT
                / "plugins"
                / "project-management-workflows"
                / "skills"
                / skill_name
                / "agents"
                / "openai.yaml"
            )
            payload = yaml.safe_load(metadata_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["interface"]["default_prompt"].count(f"${skill_name}"), 1)
            self.assertTrue(
                any(tool["value"] == "github" for tool in payload["dependencies"]["tools"])
            )

    def test_repo_local_skill_runtime_delegates_to_external_cli(self) -> None:
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
        self.assertIn("agent_tools.cli", output.getvalue())
        self.assertIn("research digest", output.getvalue())
        self.assertIn("intake-preview", SKILL_SPECS)
        self.assertIn("digest", ACTION_SPECS)
        self.assertIn("plan-to-backlog-preview", SKILL_SPECS)
        self.assertIn("plan-to-backlog", ACTION_SPECS)
