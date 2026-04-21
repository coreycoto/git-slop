from __future__ import annotations

import json
import unittest
from datetime import date
from pathlib import Path

from agent_tools.github.governance.milestone_check import build_milestone_check_payload
from agent_tools.github.shared.issue_catalog import load_issue_seed_catalog
from agent_tools.github.shared.label_palette import load_label_palette, repo_managed_labels
from agent_tools.github.shared.project_config import load_project_config

REPO_ROOT = Path(__file__).resolve().parents[3]


class GovernanceConfigTests(unittest.TestCase):
    def test_project_config_declares_expected_views_and_fields(self) -> None:
        payload = load_project_config(REPO_ROOT)
        self.assertEqual(payload["backlog_project"]["title"], "git-slop")
        self.assertEqual([view["name"] for view in payload["views"]], ["Backlog", "Epics"])
        self.assertEqual(
            [field["name"] for field in payload["fields"]],
            ["Status", "Priority", "Queue Order"],
        )

    def test_label_palette_keeps_repo_managed_subset_sparse(self) -> None:
        payload = load_label_palette(REPO_ROOT)
        self.assertEqual(
            sorted(label["name"] for label in repo_managed_labels(payload)),
            ["epic", "maintenance"],
        )

    def test_issue_seed_catalog_keeps_policy_data_repo_local(self) -> None:
        payload = load_issue_seed_catalog(REPO_ROOT)
        self.assertEqual([epic["priority"] for epic in payload["epics"]], ["Now", "Next", "Later"])
        self.assertTrue(all(epic["queue_order"] is None for epic in payload["epics"]))
        self.assertEqual(payload["queue_items"][0]["queue_order"], 10)

    def test_milestone_check_detects_missing_next_quarter(self) -> None:
        existing_payload_path = (
            REPO_ROOT / "tests" / "fixtures" / "github" / "existing_milestones.json"
        )
        payload = build_milestone_check_payload(
            today=date(2026, 4, 21),
            existing_payload=json.loads(existing_payload_path.read_text(encoding="utf-8")),
        )
        self.assertIn("2026 Q3", payload["drift"]["missing_titles"])
