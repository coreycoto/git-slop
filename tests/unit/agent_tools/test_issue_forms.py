from __future__ import annotations

import unittest
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover
    yaml = None

REPO_ROOT = Path(__file__).resolve().parents[3]


@unittest.skipIf(yaml is None, "PyYAML is required for issue form validation tests.")
class IssueFormTests(unittest.TestCase):
    def test_issue_forms_use_expected_prefixes_and_labels(self) -> None:
        expected = {
            "epic.yml": ("Epic: ", "epic"),
            "maintenance.yml": ("Maintenance: ", "maintenance"),
            "enhancement.yml": ("Enhancement: ", "enhancement"),
            "research.yml": ("Research: ", "question"),
            "bug.yml": ("Bug: ", "bug"),
        }
        for filename, (prefix, label) in expected.items():
            with self.subTest(filename=filename):
                payload = yaml.safe_load(
                    (REPO_ROOT / ".github" / "ISSUE_TEMPLATE" / filename).read_text()
                )
                self.assertEqual(payload["title"], prefix)
                self.assertIn(label, payload["labels"])
                self.assertTrue(payload["body"])

    def test_issue_template_config_points_to_contributing_guide(self) -> None:
        payload = yaml.safe_load(
            (REPO_ROOT / ".github" / "ISSUE_TEMPLATE" / "config.yml").read_text()
        )
        links = payload["contact_links"]
        self.assertEqual(links[0]["name"], "Contributing Guide")
        self.assertIn("CONTRIBUTING.md", links[0]["url"])
