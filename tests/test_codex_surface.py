from __future__ import annotations

import json
import unittest
from pathlib import Path

from git_slop.integrations.agents.codex_surface import (
    AGENT_SKILL_BINDINGS,
    PLUGIN_SKILL_CATALOG,
    ROOT_AGENTS,
    WORKFLOW_ASSETS,
    validate_codex_surface,
)
from git_slop.integrations.agents.skills import SKILL_SPECS

REPO_ROOT = Path(__file__).resolve().parents[1]


class CodexSurfaceTests(unittest.TestCase):
    def test_codex_surface_validation_passes(self) -> None:
        self.assertEqual(validate_codex_surface(REPO_ROOT), [])

    def test_plugin_catalog_matches_expected_skill_names(self) -> None:
        self.assertEqual(sorted(PLUGIN_SKILL_CATALOG), [
            "dependency-remediation",
            "docs-taxonomy",
            "ensure-quarter-milestones",
            "github-backlog-mutate",
            "intake",
            "intake-preview",
            "label-palette-design",
            "merge-on-green",
            "plan-quarter-apply",
            "plan-quarter-preview",
            "plan-to-backlog-preview",
            "release-publish",
            "review-to-backlog-apply",
            "review-to-backlog-preview",
        ])
        self.assertTrue(set(SKILL_SPECS).issubset(PLUGIN_SKILL_CATALOG))

    def test_expected_custom_agents_and_workflow_assets_exist(self) -> None:
        self.assertEqual(sorted(ROOT_AGENTS), [
            "dependency_patcher",
            "docs_taxonomist",
            "governance_auditor",
            "merge_gatekeeper",
            "release_publisher",
        ])
        self.assertEqual(sorted(WORKFLOW_ASSETS), [
            "dependency-remediation.yml",
            "docs-taxonomy.yml",
            "governance-reconcile.yml",
            "merge-on-green.yml",
            "release-publish.yml",
        ])

    def test_custom_agents_bind_exact_plugin_skills(self) -> None:
        for agent_name, relative_path in ROOT_AGENTS.items():
            payload = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
            self.assertNotIn('.agents/skills/', payload)
            for expected_skill in AGENT_SKILL_BINDINGS[agent_name]:
                self.assertIn(expected_skill, payload)

    def test_marketplace_installs_plugin_by_default(self) -> None:
        marketplace = json.loads(
            (REPO_ROOT / ".agents" / "plugins" / "marketplace.json").read_text(
                encoding="utf-8"
            )
        )
        plugin = next(
            item
            for item in marketplace["plugins"]
            if item["name"] == "project-management-workflows"
        )
        self.assertEqual(plugin["policy"]["installation"], "INSTALLED_BY_DEFAULT")


if __name__ == "__main__":
    unittest.main()
