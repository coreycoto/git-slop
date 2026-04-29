from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from git_slop.reports.plan import build_plan_payload, render_plan_text

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


def make_file_record(path: str, priority_score: float) -> dict[str, object]:
    return {
        "path": path,
        "priority_score": priority_score,
        "priority_band": "watchlist",
        "context_band": "healthy",
        "reason_codes": [],
        "costs": {},
        "overlays": {},
    }


def make_folder_record(path: str, priority_score: float) -> dict[str, object]:
    return {
        "path": path,
        "priority_score": priority_score,
        "priority_band": "watchlist",
        "context_band": "healthy",
        "reason_codes": [],
        "costs": {},
        "overlays": {},
    }


class PlanCommandTests(unittest.TestCase):
    def test_plan_requires_one_selector(self) -> None:
        report = FIXTURE_DIR / "local_repo_folder_report.json"

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
        report = FIXTURE_DIR / "local_repo_folder_report.json"

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
        report = FIXTURE_DIR / "relationship_focused_report.json"
        expected_text = (FIXTURE_DIR / "relationship_focused_plan.txt").read_text(
            encoding="utf-8"
        )
        expected_json = json.loads(
            (FIXTURE_DIR / "relationship_focused_plan.json").read_text(encoding="utf-8")
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

    def test_folder_selector_suppresses_weak_subset_slices(self) -> None:
        report = {
            "schema_version": 3,
            "files": [
                make_file_record("src/pkg/a.py", 60.0),
                make_file_record("src/pkg/b.py", 59.0),
                make_file_record("src/pkg/c.py", 58.0),
            ],
            "folders": [make_folder_record("src/pkg", 61.0)],
            "action_queue": [
                {"path": "src/pkg/a.py"},
                {"path": "src/pkg/b.py"},
                {"path": "src/pkg/c.py"},
            ],
            "overlays": {
                "organization_health": {
                    "relationships": {
                        "duplicate_neighborhoods": [],
                        "near_duplicate_neighborhoods": [],
                        "temporal_coupling_edges": [
                            {
                                "id": "rel-a-b",
                                "kind": "temporal_coupling_edge",
                                "source_path": "src/pkg/a.py",
                                "target_path": "src/pkg/b.py",
                                "evidence_score": 10.0,
                                "crosses_top_level_boundary": False,
                            }
                        ],
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

        payload = build_plan_payload(report, path="src/pkg")

        self.assertEqual(len(payload["proposed_slices"]), 1)
        self.assertEqual(
            payload["proposed_slices"][0]["scope_paths"],
            ["src/pkg/a.py", "src/pkg/b.py", "src/pkg/c.py"],
        )

    def test_relationship_selector_skips_spill_heavy_cluster_followups(self) -> None:
        member_paths = [f"src/pkg/{name}.py" for name in "abcdefghijkl"]
        report = {
            "schema_version": 3,
            "files": [
                make_file_record(path, 100.0 - index)
                for index, path in enumerate(member_paths)
            ],
            "folders": [],
            "action_queue": [],
            "overlays": {
                "organization_health": {
                    "relationships": {
                        "duplicate_neighborhoods": [],
                        "near_duplicate_neighborhoods": [
                            {
                                "id": "rel-a-b",
                                "kind": "near_duplicate_neighborhood",
                                "source_path": "src/pkg/a.py",
                                "target_path": "src/pkg/b.py",
                                "evidence_score": 120.0,
                                "crosses_top_level_boundary": False,
                            }
                        ],
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
                                "member_paths": member_paths,
                                "member_count": len(member_paths),
                                "top_level_roots": ["src"],
                                "evidence_score": 150.0,
                                "source_relationship_ids": ["rel-a-b"],
                                "candidate_type": "reduce_scattered_concept",
                            }
                        ],
                        "boundary_leakage_clusters": [],
                        "consolidation_candidates": [],
                    },
                }
            },
        }

        payload = build_plan_payload(report, relationship_id="rel-a-b")

        self.assertEqual(len(payload["proposed_slices"]), 1)
        self.assertEqual(
            payload["proposed_slices"][0]["scope_paths"],
            ["src/pkg/a.py", "src/pkg/b.py"],
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

    def test_text_rendering_truncates_evidence_and_out_of_scope(self) -> None:
        text = render_plan_text(
            {
                "target": {"kind": "path", "path": "src/pkg", "record_type": "folder"},
                "proposed_slices": [
                    {
                        "title": "Inspect slice",
                        "scope_paths": ["src/pkg/a.py", "src/pkg/b.py"],
                        "why_this_slice": "Keep the proposal reviewable.",
                        "supporting_relationship_ids": [
                            "rel-1",
                            "rel-2",
                            "rel-3",
                            "rel-4",
                        ],
                        "supporting_cluster_ids": [
                            "cluster-1",
                            "cluster-2",
                            "cluster-3",
                        ],
                        "out_of_scope_paths": [
                            "src/pkg/c.py",
                            "src/pkg/d.py",
                            "src/pkg/e.py",
                            "src/pkg/f.py",
                            "src/pkg/g.py",
                            "src/pkg/h.py",
                        ],
                    }
                ],
                "boundary_note": "Plan boundary.",
            }
        )

        self.assertIn("relationships=rel-1, rel-2, rel-3 (+1 more)", text)
        self.assertIn("clusters=cluster-1, cluster-2 (+1 more)", text)
        self.assertIn(
            "out_of_scope: src/pkg/c.py, src/pkg/d.py, src/pkg/e.py, "
            "src/pkg/f.py, src/pkg/g.py (+1 more)",
            text,
        )

    def test_plan_json_output_keeps_current_schema_and_key_set(self) -> None:
        report = json.loads(
            (FIXTURE_DIR / "local_repo_folder_report.json").read_text(encoding="utf-8")
        )

        payload = build_plan_payload(report, path="src/git_slop")

        self.assertEqual(
            set(payload),
            {
                "schema_version",
                "report_schema_version",
                "command",
                "selector",
                "target",
                "proposed_slices",
                "ranking_basis",
                "backlog_handoff",
                "boundary_note",
            },
        )
        self.assertEqual(payload["schema_version"], 2)
        self.assertEqual(payload["report_schema_version"], 3)
        self.assertEqual(payload["backlog_handoff"]["mutation_policy"], "preview_only")
        self.assertEqual(
            payload["backlog_handoff"]["target_plugin_skill"],
            "$project-management-workflows:plan-to-backlog-preview",
        )
        self.assertTrue(payload["proposed_slices"])
        self.assertEqual(
            set(payload["proposed_slices"][0]),
            {
                "id",
                "title",
                "scope_paths",
                "out_of_scope_paths",
                "supporting_relationship_ids",
                "supporting_cluster_ids",
                "evidence_summary",
                "backlog_handoff",
                "why_this_slice",
                "ranking_reason",
            },
        )
        self.assertEqual(
            payload["proposed_slices"][0]["backlog_handoff"]["mutation_policy"],
            "preview_only",
        )

    def test_plan_supports_file_and_cluster_selectors(self) -> None:
        report = FIXTURE_DIR / "relationship_focused_report.json"

        file_completed = run_cli(
            "plan",
            "--report",
            str(report),
            "--path",
            "src/consumer_toolkit/github/current_repo.py",
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
        self.assertEqual(
            file_payload["target"]["path"],
            "src/consumer_toolkit/github/current_repo.py",
        )
        self.assertEqual(cluster_payload["target"]["id"], "duplicate_set-ce293b441009")
        self.assertEqual(file_payload["report_schema_version"], 3)
        self.assertEqual(cluster_payload["report_schema_version"], 3)
