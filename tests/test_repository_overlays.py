from __future__ import annotations

import json
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]


class RepositoryOverlayTests(unittest.TestCase):
    def test_project_config_declares_git_slop_views_and_fields(self) -> None:
        payload = json.loads(
            (REPO_ROOT / "config" / "github" / "project_config.json").read_text(
                encoding="utf-8"
            )
        )

        self.assertEqual(payload["backlog_project"]["title"], "git-slop")
        self.assertEqual(
            [view["name"] for view in payload["views"]],
            ["Backlog", "Epics"],
        )
        self.assertEqual(
            [field["name"] for field in payload["fields"]],
            ["Status", "Priority", "Queue Order"],
        )

    def test_label_palette_keeps_repo_managed_subset_sparse(self) -> None:
        payload = json.loads(
            (REPO_ROOT / "config" / "labels" / "label_palette.json").read_text(
                encoding="utf-8"
            )
        )

        self.assertEqual(
            sorted(
                label["name"]
                for label in payload["labels"]
                if label["owner"] == "repo-managed"
            ),
            ["epic", "maintenance"],
        )


if __name__ == "__main__":
    unittest.main()
