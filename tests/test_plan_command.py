from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from git_slop.reports.plan import build_plan_payload

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


class PlanCommandTests(unittest.TestCase):
    def test_plan_requires_one_selector(self) -> None:
        report = FIXTURE_DIR / "git_slop_folder_report.json"

        missing_selector = run_cli("plan", "--report", str(report))
        multiple_selectors = run_cli(
            "plan",
            "--report",
            str(report),
            "--path",
            "src/git_slop",
            "--cluster",
            "scattered_concept-c1c73fb5da90",
        )

        self.assertEqual(missing_selector.returncode, 2)
        self.assertIn("usage:", missing_selector.stderr)
        self.assertEqual(multiple_selectors.returncode, 2)
        self.assertIn("usage:", multiple_selectors.stderr)

    def test_plan_rejects_non_schema3_report(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            report_path = Path(tmp_dir) / "report.json"
            report_path.write_text(json.dumps({"schema_version": 2}), encoding="utf-8")

            completed = run_cli("plan", "--report", str(report_path), "--path", "README.md")

            self.assertEqual(completed.returncode, 2)
            self.assertIn("requires report schema 3", completed.stdout)

    def test_plan_folder_fixture_is_deterministic_and_bounded(self) -> None:
        report = FIXTURE_DIR / "git_slop_folder_report.json"

        first = run_cli(
            "plan",
            "--report",
            str(report),
            "--path",
            "src/git_slop",
            "--format",
            "json",
        )
        second = run_cli(
            "plan",
            "--report",
            str(report),
            "--path",
            "src/git_slop",
            "--format",
            "json",
        )

        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(first.stdout, second.stdout)

        payload = json.loads(first.stdout)
        self.assertEqual(payload["command"], "plan")
        self.assertEqual(payload["selector"]["kind"], "path")
        self.assertEqual(payload["target"]["record_type"], "folder")
        self.assertEqual(len(payload["proposed_slices"]), 3)
        self.assertTrue(all(len(item["scope_paths"]) <= 5 for item in payload["proposed_slices"]))
        self.assertEqual(
            payload["proposed_slices"][0]["scope_paths"],
            [
                "src/git_slop/organization.py",
                "src/git_slop/reporting.py",
                "src/git_slop/history.py",
                "src/git_slop/__init__.py",
                "src/git_slop/cli.py",
            ],
        )

    def test_plan_relationship_fixture_matches_text_snapshot_and_json(self) -> None:
        report = FIXTURE_DIR / "agent_tools_relationship_report.json"
        expected_text = (FIXTURE_DIR / "agent_tools_relationship_plan.txt").read_text(
            encoding="utf-8"
        )
        expected_json = json.loads(
            (FIXTURE_DIR / "agent_tools_relationship_plan.json").read_text(encoding="utf-8")
        )

        completed = run_cli(
            "plan",
            "--report",
            str(report),
            "--relationship",
            "near_duplicate_neighborhood-35e7fad1c4e0",
        )
        json_completed = run_cli(
            "plan",
            "--report",
            str(report),
            "--relationship",
            "near_duplicate_neighborhood-35e7fad1c4e0",
            "--format",
            "json",
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(json_completed.returncode, 0, json_completed.stderr)
        self.assertEqual(completed.stdout, expected_text)
        self.assertEqual(json.loads(json_completed.stdout), expected_json)
        self.assertEqual(len(json.loads(json_completed.stdout)["proposed_slices"]), 1)
        self.assertTrue(
            all(
                len(item["scope_paths"]) <= 5
                for item in json.loads(json_completed.stdout)["proposed_slices"]
            )
        )

    def test_broad_cluster_plan_starts_with_tight_relationship_backed_slice(self) -> None:
        report = {
            "schema_version": 3,
            "files": [
                {
                    "path": "src/focus/a.py",
                    "priority_score": 50.0,
                    "priority_band": "needs_refactor",
                    "context_band": "healthy",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                },
                {
                    "path": "src/focus/b.py",
                    "priority_score": 49.0,
                    "priority_band": "watchlist",
                    "context_band": "healthy",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                },
                {
                    "path": "src/focus/c.py",
                    "priority_score": 48.0,
                    "priority_band": "watchlist",
                    "context_band": "healthy",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                },
                {
                    "path": "src/focus/d.py",
                    "priority_score": 47.0,
                    "priority_band": "watchlist",
                    "context_band": "healthy",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                },
                {
                    "path": "src/focus/e.py",
                    "priority_score": 46.0,
                    "priority_band": "watchlist",
                    "context_band": "healthy",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                },
                {
                    "path": "src/focus/f.py",
                    "priority_score": 45.0,
                    "priority_band": "watchlist",
                    "context_band": "healthy",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                },
            ],
            "folders": [],
            "action_queue": [],
            "overlays": {
                "organization_health": {
                    "relationships": {
                        "duplicate_neighborhoods": [
                            {
                                "id": "dup-strong",
                                "kind": "duplicate_neighborhood",
                                "source_path": "src/focus/a.py",
                                "target_path": "src/focus/b.py",
                                "evidence_score": 100.0,
                                "crosses_top_level_boundary": False,
                            }
                        ],
                        "near_duplicate_neighborhoods": [],
                        "temporal_coupling_edges": [],
                        "lexical_affinity_edges": [],
                        "boundary_leakage_edges": [],
                    },
                    "clusters": {
                        "duplicate_sets": [],
                        "scattered_concepts": [
                            {
                                "id": "scatter-wide",
                                "kind": "scattered_concept",
                                "member_paths": [
                                    "src/focus/a.py",
                                    "src/focus/b.py",
                                    "src/focus/c.py",
                                    "src/focus/d.py",
                                    "src/focus/e.py",
                                    "src/focus/f.py",
                                ],
                                "member_count": 40,
                                "top_level_roots": ["src"],
                                "evidence_score": 150.0,
                                "source_relationship_ids": ["dup-strong"],
                                "candidate_type": "reduce_scattered_concept",
                            }
                        ],
                        "boundary_leakage_clusters": [],
                        "consolidation_candidates": [],
                    },
                }
            },
        }

        payload = build_plan_payload(report, cluster_id="scatter-wide")

        self.assertEqual(
            payload["proposed_slices"][0]["scope_paths"],
            ["src/focus/a.py", "src/focus/b.py"],
        )

    def test_plan_supports_file_and_cluster_selectors(self) -> None:
        report = FIXTURE_DIR / "agent_tools_relationship_report.json"

        file_completed = run_cli(
            "plan",
            "--report",
            str(report),
            "--path",
            "src/agent_tools/github/current_repo.py",
            "--format",
            "json",
        )
        cluster_completed = run_cli(
            "plan",
            "--report",
            str(report),
            "--cluster",
            "duplicate_set-ce293b441009",
            "--format",
            "json",
        )

        self.assertEqual(file_completed.returncode, 0, file_completed.stderr)
        self.assertEqual(cluster_completed.returncode, 0, cluster_completed.stderr)

        file_payload = json.loads(file_completed.stdout)
        cluster_payload = json.loads(cluster_completed.stdout)
        self.assertEqual(file_payload["target"]["path"], "src/agent_tools/github/current_repo.py")
        self.assertEqual(cluster_payload["target"]["id"], "duplicate_set-ce293b441009")
        self.assertEqual(file_payload["report_schema_version"], 3)
        self.assertEqual(cluster_payload["report_schema_version"], 3)
