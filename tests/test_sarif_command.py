from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from git_slop.reports.sarif import build_sarif_payload

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


class SarifCommandTests(unittest.TestCase):
    def test_sarif_payload_exports_action_queue_findings(self) -> None:
        report = load_fixture("large_repo_top_report.json")

        payload = build_sarif_payload(report, report_path="report.json", top=2)

        self.assertEqual(payload["version"], "2.1.0")
        run = payload["runs"][0]
        self.assertEqual(run["tool"]["driver"]["name"], "git-slop")
        self.assertEqual(run["tool"]["driver"]["rules"][0]["id"], "git-slop.hotspot")
        self.assertEqual(len(run["results"]), 2)
        first = run["results"][0]
        self.assertEqual(first["ruleId"], "git-slop.hotspot")
        self.assertEqual(first["level"], "warning")
        self.assertEqual(
            first["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            report["action_queue"][0]["path"],
        )
        self.assertEqual(first["properties"]["git_slop"]["rank"], 1)
        self.assertIn("costs", first["properties"]["git_slop"])
        self.assertIn("strongest_overlays", first["properties"]["git_slop"])
        self.assertIn("boundary_note", run["properties"]["git_slop"])

    def test_sarif_rejects_non_schema3_reports(self) -> None:
        with self.assertRaisesRegex(ValueError, "requires report schema 3"):
            build_sarif_payload({"schema_version": 2})

    def test_sarif_cli_stdout_and_output_file(self) -> None:
        report = FIXTURE_DIR / "large_repo_top_report.json"

        stdout_completed = run_cli("sarif", "--report", str(report), "--top", "1")
        self.assertEqual(stdout_completed.returncode, 0, stdout_completed.stderr)
        stdout_payload = json.loads(stdout_completed.stdout)
        self.assertEqual(stdout_payload["version"], "2.1.0")
        self.assertEqual(len(stdout_payload["runs"][0]["results"]), 1)

        with tempfile.TemporaryDirectory() as tmp_dir:
            output_path = Path(tmp_dir) / "git-slop.sarif"
            output_completed = run_cli(
                "sarif",
                "--report",
                str(report),
                "--top",
                "1",
                "--output",
                str(output_path),
            )
            self.assertEqual(output_completed.returncode, 0, output_completed.stderr)
            self.assertIn("Wrote SARIF report", output_completed.stdout)
            file_payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(len(file_payload["runs"][0]["results"]), 1)

    def test_sarif_cli_invalid_inputs_return_usage_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            invalid = Path(tmp_dir) / "invalid.json"
            invalid.write_text(json.dumps({"schema_version": 2}), encoding="utf-8")

            missing_completed = run_cli("sarif", "--report", str(Path(tmp_dir) / "missing.json"))
            invalid_completed = run_cli("sarif", "--report", str(invalid))
            bad_top_completed = run_cli(
                "sarif",
                "--report",
                str(FIXTURE_DIR / "large_repo_top_report.json"),
                "--top",
                "0",
            )

        self.assertEqual(missing_completed.returncode, 2)
        self.assertIn("Report not found:", missing_completed.stdout)
        self.assertEqual(invalid_completed.returncode, 2)
        self.assertIn("requires report schema 3", invalid_completed.stdout)
        self.assertEqual(bad_top_completed.returncode, 2)
        self.assertIn("--top must be greater than zero", bad_top_completed.stdout)


if __name__ == "__main__":
    unittest.main()
