from __future__ import annotations

import json
import unittest
from pathlib import Path

from git_slop.integrations.agents.codex_surface import (
    AGENT_SKILL_REFERENCES,
    EXPECTED_MARKETPLACE_NAME,
    EXPECTED_PLUGIN_SHA,
    EXPECTED_PLUGIN_URL,
    MARKETPLACE_SOURCE_MANIFEST,
    REMOVED_LOCAL_PLUGIN_ROOT,
    ROOT_AGENTS,
    WORKFLOW_ASSETS,
    validate_codex_surface,
)

REPO_ROOT = Path(__file__).resolve().parents[1]


class CodexSurfaceTests(unittest.TestCase):
    def test_codex_surface_validation_passes(self) -> None:
        self.assertEqual(validate_codex_surface(REPO_ROOT), [])

    def test_expected_custom_agents_and_workflow_assets_exist(self) -> None:
        self.assertEqual(
            sorted(ROOT_AGENTS),
            [
                "dependency_patcher",
                "docs_taxonomist",
                "governance_auditor",
                "merge_gatekeeper",
                "release_publisher",
            ],
        )
        self.assertEqual(
            sorted(WORKFLOW_ASSETS),
            [
                "dependency-remediation.yml",
                "docs-taxonomy.yml",
                "governance-reconcile.yml",
                "merge-on-green.yml",
                "release-publish.yml",
            ],
        )

    def test_custom_agents_reference_installed_skill_names_only(self) -> None:
        for agent_name, relative_path in ROOT_AGENTS.items():
            payload = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
            self.assertNotIn("[[skills.config]]", payload)
            self.assertNotIn("plugins/project-management-workflows/", payload)
            for expected_skill in AGENT_SKILL_REFERENCES[agent_name]:
                self.assertIn(expected_skill, payload)

    def test_marketplace_source_manifest_pins_publisher_sha(self) -> None:
        manifest = json.loads(
            (REPO_ROOT / MARKETPLACE_SOURCE_MANIFEST).read_text(encoding="utf-8")
        )
        self.assertEqual(manifest["marketplace_name"], EXPECTED_MARKETPLACE_NAME)
        self.assertEqual(manifest["source_url"], EXPECTED_PLUGIN_URL)
        self.assertEqual(manifest["ref"], EXPECTED_PLUGIN_SHA)
        self.assertEqual(manifest["required_plugin"], "project-management-workflows")

    def test_local_plugin_tree_is_removed(self) -> None:
        self.assertFalse((REPO_ROOT / REMOVED_LOCAL_PLUGIN_ROOT).exists())


if __name__ == "__main__":
    unittest.main()
