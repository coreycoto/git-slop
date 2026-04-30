from __future__ import annotations

import contextlib
import importlib.util
import io
import subprocess
import sys
import unittest
from pathlib import Path

from git_slop.agent_skill_runtime import run_skill_entrypoint
from git_slop.agent_skills import ACTION_SPECS, SKILL_SPECS
from git_slop.integrations.agents.codex_surface import PLUGIN_SKILL_CATALOG

REPO_ROOT = Path(__file__).resolve().parents[1]
PROJECT_SNAPSHOT_ENTRYPOINT = (
    "from agent_plugins.github.shared.project_snapshot import main; raise SystemExit(main())"
)
AGENT_PLUGINS_AVAILABLE = importlib.util.find_spec("agent_plugins") is not None


class AgentPluginsRuntimeIntegrationTests(unittest.TestCase):
    @unittest.skipUnless(
        AGENT_PLUGINS_AVAILABLE,
        "agent-plugins optional dependency is unavailable.",
    )
    def test_external_agent_plugins_runtime_is_available(self) -> None:
        completed = subprocess.run(
            [
                sys.executable,
                "-c",
                PROJECT_SNAPSHOT_ENTRYPOINT,
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
        self.assertIn("docs-taxonomy", PLUGIN_SKILL_CATALOG)
        self.assertIn("plan-to-backlog-preview", PLUGIN_SKILL_CATALOG)

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
        self.assertIn("agent_plugins.research.digest", output.getvalue())
        self.assertIn("docs/vision.md", output.getvalue())
        self.assertIn("intake-preview", SKILL_SPECS)
        self.assertIn("digest", ACTION_SPECS)
        self.assertIn("plan-to-backlog-preview", SKILL_SPECS)
        self.assertIn("plan-to-backlog", ACTION_SPECS)
