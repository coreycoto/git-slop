from __future__ import annotations

import importlib.util
import json
import unittest
from datetime import date
from pathlib import Path

AGENT_PLUGINS_AVAILABLE = importlib.util.find_spec("agent_plugins") is not None

if AGENT_PLUGINS_AVAILABLE:
    from agent_plugins.github.governance.milestone_check import build_milestone_check_payload
    from agent_plugins.github.shared.label_palette import load_label_palette, repo_managed_labels
    from agent_plugins.github.shared.project_config import load_project_config

REPO_ROOT = Path(__file__).resolve().parents[3]


@unittest.skipUnless(AGENT_PLUGINS_AVAILABLE, "agent-plugins optional dependency is unavailable.")
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

    def test_milestone_check_detects_missing_next_quarter(self) -> None:
        existing_payload_path = (
            REPO_ROOT / "tests" / "fixtures" / "github" / "existing_milestones.json"
        )
        payload = build_milestone_check_payload(
            today=date(2026, 4, 21),
            existing_payload=json.loads(existing_payload_path.read_text(encoding="utf-8")),
        )
        self.assertIn("2026 Q3", payload["drift"]["missing_titles"])
