from __future__ import annotations

import json
import unittest
from pathlib import Path

from agent_tools.github.governance.validate_backlog_mutations import (
    normalize_backlog_mutation_payload,
)
from agent_tools.github.planning.build_quarter_plan_delta import build_quarter_plan_delta
from agent_tools.github.planning.validate_quarter_plan import normalize_quarter_plan_payload
from agent_tools.github.reviews.review_to_backlog import build_review_backlog_delta

REPO_ROOT = Path(__file__).resolve().parents[3]


class BacklogDeltaTests(unittest.TestCase):
    def test_backlog_mutation_validation_normalizes_titles(self) -> None:
        payload = json.loads(
            (
                REPO_ROOT / "tests" / "fixtures" / "github" / "backlog_mutations.json"
            ).read_text()
        )
        normalized = normalize_backlog_mutation_payload(payload)
        self.assertEqual(
            normalized["issues"][0]["title"],
            "Enhancement: build tracked-file inventory",
        )
        self.assertEqual(
            normalized["issues"][1]["title"],
            "Maintenance: add dogfood CI workflow",
        )

    def test_review_findings_build_backlog_delta(self) -> None:
        payload = json.loads(
            (
                REPO_ROOT / "tests" / "fixtures" / "github" / "review_findings.json"
            ).read_text()
        )
        delta = build_review_backlog_delta(payload)
        titles = [issue["title"] for issue in delta["issues"]]
        self.assertIn(
            "Bug: tracked-file inventory should skip missing tracked paths",
            titles,
        )
        self.assertIn(
            "Maintenance: document the dogfood workflow guardrails",
            titles,
        )

    def test_quarter_plan_builds_milestone_delta(self) -> None:
        payload = json.loads(
            (REPO_ROOT / "tests" / "fixtures" / "github" / "quarter_plan.json").read_text()
        )
        normalized = normalize_quarter_plan_payload(payload)
        delta = build_quarter_plan_delta(normalized)
        self.assertEqual(len(delta["issues"]), 2)
        self.assertTrue(all(issue["milestone"] == "2026 Q2" for issue in delta["issues"]))
        self.assertEqual(delta["issues"][1]["type"], "Maintenance")
