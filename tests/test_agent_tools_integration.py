from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
PROJECT_SNAPSHOT_ENTRYPOINT = (
    "from agent_plugins.github.shared.project_snapshot import main; raise SystemExit(main())"
)
AGENT_PLUGINS_AVAILABLE = importlib.util.find_spec("agent_plugins") is not None


class AgentPluginsIntegrationTests(unittest.TestCase):
    @unittest.skipUnless(
        AGENT_PLUGINS_AVAILABLE,
        "agent-plugins optional dependency is unavailable.",
    )
    def test_external_agent_plugins_runtime_is_callable_without_network(self) -> None:
        completed = subprocess.run(
            [
                sys.executable,
                "-c",
                PROJECT_SNAPSHOT_ENTRYPOINT,
                "--help",
            ],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("Snapshot a GitHub project", completed.stdout)
        self.assertIn("--require-live", completed.stdout)

    def test_repo_local_validator_imports_without_agent_plugins(self) -> None:
        script = """
from git_slop.integrations import agents
assert agents.__all__ == ["validate_codex_surface"]
assert callable(agents.validate_codex_surface)
"""
        env = {**os.environ, "PYTHONPATH": str(REPO_ROOT / "src")}
        completed = subprocess.run(
            [sys.executable, "-S", "-c", script],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
