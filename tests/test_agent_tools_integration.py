from __future__ import annotations

import contextlib
import io
import subprocess
import sys
import unittest
from pathlib import Path

from agent_tools.skills.metadata import (
    build_expected_outputs,
    load_and_validate_skill_metadata_manifest,
)

from git_slop.agent_skill_runtime import run_skill_entrypoint
from git_slop.agent_skills import ACTION_SPECS, SKILL_SPECS

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

    def test_skill_metadata_manifest_sync_contract_still_holds(self) -> None:
        manifest = load_and_validate_skill_metadata_manifest(
            REPO_ROOT / "config" / "agents" / "skill_metadata_manifest.json",
            repo_root=REPO_ROOT,
            skills_root=REPO_ROOT / ".agents" / "skills",
        )
        expected_outputs = build_expected_outputs(manifest)

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
        self.assertIn(
            REPO_ROOT / ".agents" / "skills" / "intake" / "agents" / "openai.yaml",
            expected_outputs,
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
        self.assertIn("intake-preview", SKILL_SPECS)
        self.assertIn("digest", ACTION_SPECS)
