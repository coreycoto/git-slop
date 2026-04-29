from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from git_slop.reports.compare import build_compare_payload

REPO_ROOT = Path(__file__).resolve().parents[1]
SRC_DIR = REPO_ROOT / "src"
FIXTURE_DIR = REPO_ROOT / "tests" / "fixtures" / "reports"


def run_cli(*args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        str(SRC_DIR) if not existing_pythonpath else f"{SRC_DIR}{os.pathsep}{existing_pythonpath}"
    )
    return subprocess.run(
        [sys.executable, "-m", "git_slop", *args],
        cwd=REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )


def load_fixture(name: str) -> dict[str, object]:
    return json.loads((FIXTURE_DIR / name).read_text(encoding="utf-8"))


class CompareCommandTests(unittest.TestCase):
    def test_compare_payload_classifies_records_and_queue_movement(self) -> None:
        payload = build_compare_payload(
            load_fixture("compare_base_report.json"),
            load_fixture("compare_head_report.json"),
            base_path="base.json",
            head_path="head.json",
        )

        self.assertEqual(payload["schema_version"], 1)
        self.assertEqual(payload["command"], "compare")
        file_statuses = {item["path"]: item["status"] for item in payload["file_deltas"]}
        self.assertEqual(file_statuses["src/a.py"], "changed")
        self.assertEqual(file_statuses["src/b.py"], "changed")
        self.assertEqual(file_statuses["src/new.py"], "added")
        self.assertEqual(file_statuses["src/removed.py"], "removed")
        a_delta = next(item for item in payload["file_deltas"] if item["path"] == "src/a.py")
        self.assertEqual(a_delta["priority_score_delta"], 50.0)
        self.assertEqual(a_delta["token_delta"], 60)
        self.assertEqual(a_delta["context_band_delta"], 2)
        self.assertEqual(a_delta["priority_band_delta"], 2)
        self.assertEqual(a_delta["overlay_deltas"][0]["label"], "verification")
        self.assertEqual(a_delta["overlay_deltas"][0]["delta"], 0.6)
        movement = {item["path"]: item["status"] for item in payload["queue_movement"]}
        self.assertEqual(movement["src/new.py"], "newly_queued")
        self.assertEqual(movement["src/a.py"], "moved_down")
        self.assertEqual(movement["src/b.py"], "moved_down")

    def test_compare_rejects_non_schema3_reports(self) -> None:
        with self.assertRaisesRegex(ValueError, "base report must use schema 3"):
            build_compare_payload({"schema_version": 2}, load_fixture("compare_head_report.json"))

    def test_compare_cli_text_and_json_outputs(self) -> None:
        base = FIXTURE_DIR / "compare_base_report.json"
        head = FIXTURE_DIR / "compare_head_report.json"

        text_completed = run_cli("compare", "--base", str(base), "--head", str(head), "--top", "2")
        json_completed = run_cli(
            "compare",
            "--base",
            str(base),
            "--head",
            str(head),
            "--format",
            "json",
        )

        self.assertEqual(text_completed.returncode, 0, text_completed.stderr)
        self.assertIn(
            "Compare: compare_base_report.json -> compare_head_report.json",
            text_completed.stdout,
        )
        self.assertIn("Top Worsened Files", text_completed.stdout)
        self.assertIn("Queue Movement", text_completed.stdout)
        self.assertEqual(json_completed.returncode, 0, json_completed.stderr)
        payload = json.loads(json_completed.stdout)
        self.assertEqual(payload["command"], "compare")
        self.assertEqual(payload["summary"]["files"]["added"], 1)

    def test_compare_cli_missing_or_invalid_report_returns_usage_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            invalid = Path(tmp_dir) / "invalid.json"
            invalid.write_text(json.dumps({"schema_version": 2}), encoding="utf-8")
            missing_completed = run_cli(
                "compare",
                "--base",
                str(FIXTURE_DIR / "compare_base_report.json"),
                "--head",
                str(Path(tmp_dir) / "missing.json"),
            )
            invalid_completed = run_cli(
                "compare",
                "--base",
                str(invalid),
                "--head",
                str(FIXTURE_DIR / "compare_head_report.json"),
            )

        self.assertEqual(missing_completed.returncode, 2)
        self.assertIn("Report not found:", missing_completed.stdout)
        self.assertEqual(invalid_completed.returncode, 2)
        self.assertIn("base report must use schema 3", invalid_completed.stdout)

    def test_compare_cli_requires_base_and_head(self) -> None:
        completed = run_cli("compare", "--base", str(FIXTURE_DIR / "compare_base_report.json"))

        self.assertEqual(completed.returncode, 2)
        self.assertIn("usage:", completed.stderr)


if __name__ == "__main__":
    unittest.main()
