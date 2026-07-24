from __future__ import annotations

import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]


class WorkflowContractTests(unittest.TestCase):
    def test_dogfood_artifact_is_summary_only_and_short_lived(self) -> None:
        workflow = (REPO_ROOT / ".github" / "workflows" / "dogfood.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("path: .slop/latest/summary.md", workflow)
        self.assertNotIn("path: .slop/latest\n", workflow)
        self.assertIn("retention-days: 14", workflow)
