from __future__ import annotations

import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

from git_slop.reports.explain import build_explain_payload, render_explain_text

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


class ExplainRealReportFixtureTests(unittest.TestCase):
    def test_folder_explain_prefers_tight_local_clusters_over_broad_memberships(self) -> None:
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
                    "path": "src/other/z.py",
                    "priority_score": 10.0,
                    "priority_band": "watchlist",
                    "context_band": "compact",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                },
            ],
            "folders": [
                {
                    "path": "src/focus",
                    "priority_score": 50.0,
                    "priority_band": "needs_refactor",
                    "context_band": "critical",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": {},
                }
            ],
            "action_queue": [
                {"path": "src/focus/a.py"},
                {"path": "src/focus/b.py"},
            ],
            "overlays": {
                "organization_health": {
                    "relationships": {
                        "duplicate_neighborhoods": [],
                        "near_duplicate_neighborhoods": [],
                        "temporal_coupling_edges": [],
                        "lexical_affinity_edges": [],
                        "boundary_leakage_edges": [],
                    },
                    "clusters": {
                        "duplicate_sets": [
                            {
                                "id": "duplicate-small",
                                "kind": "duplicate_set",
                                "member_paths": ["src/focus/a.py", "src/focus/b.py"],
                                "member_count": 2,
                                "top_level_roots": ["src"],
                                "evidence_score": 10.0,
                                "source_relationship_ids": [],
                                "candidate_type": "consolidate_duplicate_knowledge",
                            }
                        ],
                        "scattered_concepts": [
                            {
                                "id": "scatter-large",
                                "kind": "scattered_concept",
                                "member_paths": [
                                    "src/focus/a.py",
                                    "src/focus/b.py",
                                    "src/other/z.py",
                                ],
                                "member_count": 50,
                                "top_level_roots": ["src"],
                                "evidence_score": 99.0,
                                "source_relationship_ids": [],
                                "candidate_type": "reduce_scattered_concept",
                            }
                        ],
                        "boundary_leakage_clusters": [],
                        "consolidation_candidates": [],
                    },
                }
            },
        }

        payload = build_explain_payload(report, path="src/focus")

        self.assertEqual(payload["target"]["record_type"], "folder")
        self.assertEqual(payload["supporting_clusters"][0]["id"], "duplicate-small")

    def test_local_repo_folder_fixture_matches_snapshot_and_json_additions(self) -> None:
        report = FIXTURE_DIR / "local_repo_folder_report.json"
        expected = (FIXTURE_DIR / "local_repo_folder_explain.txt").read_text(encoding="utf-8")

        completed = run_cli("explain", "--report", str(report), "--path", "src/git_slop")

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(completed.stdout, expected)

        json_completed = run_cli(
            "explain",
            "--report",
            str(report),
            "--path",
            "src/git_slop",
            "--format",
            "json",
        )
        self.assertEqual(json_completed.returncode, 0, json_completed.stderr)
        payload = json.loads(json_completed.stdout)
        self.assertEqual(payload["schema_version"], 2)
        self.assertEqual(payload["target"]["record_type"], "folder")
        self.assertEqual(len(payload["cost_summary"]["descendant_hotspots"]), 5)
        self.assertIn("descendant_overlay_maxima", payload["overlay_summary"])
        self.assertIn("evidence_summary", payload)
        self.assertIn("strongest_overlays", payload["evidence_summary"])
        self.assertEqual(
            len(payload["supporting_relationships"]),
            len({item["id"] for item in payload["supporting_relationships"]}),
        )
        self.assertEqual(
            len(payload["supporting_clusters"]),
            len({item["id"] for item in payload["supporting_clusters"]}),
        )

    def test_large_repo_top_fixture_matches_compact_snapshot(self) -> None:
        report = FIXTURE_DIR / "large_repo_top_report.json"
        expected = (FIXTURE_DIR / "large_repo_top_explain.txt").read_text(encoding="utf-8")

        completed = run_cli("explain", "--report", str(report), "--top", "5")

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(completed.stdout, expected)
        self.assertEqual(completed.stdout.count("Interpretation boundary"), 1)

        payload = json.loads(report.read_text(encoding="utf-8"))
        for index, item in enumerate(payload["action_queue"][:5], start=1):
            self.assertIn(f"{index}. {item['path']}", completed.stdout)

    def test_explain_defaults_to_top_five_when_no_selector_is_supplied(self) -> None:
        report = FIXTURE_DIR / "large_repo_top_report.json"

        completed = run_cli("explain", "--report", str(report), "--format", "json")

        self.assertEqual(completed.returncode, 0, completed.stderr)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["schema_version"], 2)
        self.assertEqual(payload["selector"], {"kind": "top", "value": 5})
        self.assertEqual(len(payload["items"]), 5)

    def test_relationship_focused_fixture_supports_relationship_selector(self) -> None:
        report = FIXTURE_DIR / "relationship_focused_report.json"

        completed = run_cli(
            "explain",
            "--report",
            str(report),
            "--relationship",
            "near_duplicate_neighborhood-35e7fad1c4e0",
            "--format",
            "json",
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["selector"]["kind"], "relationship")
        self.assertEqual(payload["schema_version"], 2)
        self.assertEqual(payload["target"]["id"], "near_duplicate_neighborhood-35e7fad1c4e0")
        self.assertIn("source", payload["cost_summary"])
        self.assertIn("target", payload["cost_summary"])
        self.assertIn("evidence_summary", payload)
        self.assertEqual(
            [cluster["id"] for cluster in payload["supporting_clusters"]],
            ["duplicate_set-ce293b441009"],
        )

    def test_relationship_text_tolerates_missing_endpoint_overlays(self) -> None:
        report = {
            "schema_version": 3,
            "files": [
                {
                    "path": "src/a.py",
                    "record_type": "file",
                    "priority_score": 42.0,
                    "priority_band": "watchlist",
                    "context_band": "healthy",
                    "reason_codes": [],
                    "costs": {},
                    "overlays": None,
                },
                {
                    "path": "src/b.py",
                    "record_type": "file",
                    "priority_score": 41.0,
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
                                "id": "dup-1",
                                "kind": "duplicate_neighborhood",
                                "source_path": "src/a.py",
                                "target_path": "src/b.py",
                                "evidence_score": 9.0,
                            }
                        ],
                        "near_duplicate_neighborhoods": [],
                        "temporal_coupling_edges": [],
                        "lexical_affinity_edges": [],
                        "boundary_leakage_edges": [],
                    },
                    "clusters": {
                        "duplicate_sets": [],
                        "scattered_concepts": [],
                        "boundary_leakage_clusters": [],
                        "consolidation_candidates": [],
                    },
                }
            },
        }

        payload = build_explain_payload(report, relationship_id="dup-1")
        rendered = render_explain_text(payload)

        self.assertIn("Explain: relationship dup-1", rendered)
        self.assertIn("- source overlays:", rendered)
        self.assertIn("- target overlays:", rendered)

    def test_git_slop_cluster_fixture_supports_cluster_selector(self) -> None:
        report = FIXTURE_DIR / "local_repo_folder_report.json"

        completed = run_cli(
            "explain",
            "--report",
            str(report),
            "--cluster",
            "scattered_concept-c1c73fb5da90",
            "--format",
            "json",
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["selector"]["kind"], "cluster")
        self.assertEqual(payload["schema_version"], 2)
        self.assertEqual(payload["target"]["id"], "scattered_concept-c1c73fb5da90")
        self.assertIn("member_hotspots", payload["cost_summary"])
        self.assertIn("evidence_summary", payload)
