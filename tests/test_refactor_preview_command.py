from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from git_slop.reports.refactor_preview import build_refactor_preview_payload

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


class RefactorPreviewCommandTests(unittest.TestCase):
    def test_refactor_preview_payload_preserves_plan_slice_evidence(self) -> None:
        plan = load_fixture("relationship_focused_plan.json")

        payload = build_refactor_preview_payload(plan, plan_path="plan.json")

        self.assertEqual(payload["schema_version"], 1)
        self.assertEqual(payload["command"], "refactor-preview")
        self.assertEqual(payload["mutation_policy"], "preview_only")
        self.assertEqual(payload["report_schema_version"], 3)
        self.assertEqual(payload["source_plan"]["schema_version"], 2)
        self.assertEqual(payload["source_plan"]["selector"], plan["selector"])
        self.assertEqual(payload["source_plan"]["target"], plan["target"])
        self.assertEqual(
            payload["source_plan"]["selected_slice_ids"],
            ["anchor-relationship-near_duplicate_neighborhood-35e7fad1c4e0"],
        )
        preview = payload["preview_slices"][0]
        source_slice = plan["proposed_slices"][0]
        self.assertEqual(preview["id"], source_slice["id"])
        self.assertEqual(preview["scope_paths"], source_slice["scope_paths"])
        self.assertEqual(preview["out_of_scope_paths"], source_slice["out_of_scope_paths"])
        self.assertEqual(
            preview["supporting_relationship_ids"],
            source_slice["supporting_relationship_ids"],
        )
        self.assertEqual(preview["supporting_cluster_ids"], source_slice["supporting_cluster_ids"])
        self.assertEqual(preview["evidence_summary"], source_slice["evidence_summary"])
        self.assertEqual(preview["backlog_handoff"], source_slice["backlog_handoff"])
        self.assertIn("No patch is generated", preview["patch_preview_notes"][0])
        self.assertIn("does not mutate code", payload["boundary_note"])

    def test_refactor_preview_filters_requested_slice_id(self) -> None:
        plan = load_fixture("relationship_focused_plan.json")
        selected = "anchor-relationship-near_duplicate_neighborhood-35e7fad1c4e0"

        payload = build_refactor_preview_payload(plan, slice_ids=[selected])

        self.assertEqual(payload["source_plan"]["selected_slice_ids"], [selected])
        self.assertEqual(len(payload["preview_slices"]), 1)
        self.assertEqual(payload["preview_slices"][0]["id"], selected)

    def test_refactor_preview_rejects_invalid_payloads_and_unknown_slice(self) -> None:
        plan = load_fixture("relationship_focused_plan.json")

        with self.assertRaisesRegex(ValueError, "requires a git slop plan payload"):
            build_refactor_preview_payload({"schema_version": 2, "command": "compare"})
        with self.assertRaisesRegex(ValueError, "requires plan schema 2"):
            build_refactor_preview_payload({"schema_version": 1, "command": "plan"})
        with self.assertRaisesRegex(ValueError, "Unknown plan slice id"):
            build_refactor_preview_payload(plan, slice_ids=["missing-slice"])

    def test_refactor_preview_cli_text_and_json_outputs(self) -> None:
        plan = FIXTURE_DIR / "relationship_focused_plan.json"
        slice_id = "anchor-relationship-near_duplicate_neighborhood-35e7fad1c4e0"

        text_completed = run_cli("refactor-preview", "--plan", str(plan), "--slice", slice_id)
        json_completed = run_cli("refactor-preview", "--plan", str(plan), "--format", "json")

        self.assertEqual(text_completed.returncode, 0, text_completed.stderr)
        self.assertIn("Refactor Preview:", text_completed.stdout)
        self.assertIn("maintainer_action:", text_completed.stdout)
        self.assertIn("scope:", text_completed.stdout)
        self.assertIn("out_of_scope:", text_completed.stdout)
        self.assertIn("evidence_summary:", text_completed.stdout)
        self.assertIn("proposed_steps:", text_completed.stdout)
        self.assertIn("Refactor preview boundary", text_completed.stdout)
        self.assertEqual(json_completed.returncode, 0, json_completed.stderr)
        payload = json.loads(json_completed.stdout)
        self.assertEqual(payload["command"], "refactor-preview")
        self.assertEqual(payload["preview_slices"][0]["mutation_policy"], "preview_only")

    def test_refactor_preview_cli_invalid_inputs_return_usage_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            invalid = Path(tmp_dir) / "invalid.json"
            invalid.write_text(
                json.dumps({"schema_version": 2, "command": "compare"}),
                encoding="utf-8",
            )
            missing_completed = run_cli(
                "refactor-preview",
                "--plan",
                str(Path(tmp_dir) / "missing.json"),
            )
            invalid_completed = run_cli("refactor-preview", "--plan", str(invalid))
            unknown_slice_completed = run_cli(
                "refactor-preview",
                "--plan",
                str(FIXTURE_DIR / "relationship_focused_plan.json"),
                "--slice",
                "missing-slice",
            )

        self.assertEqual(missing_completed.returncode, 2)
        self.assertIn("Plan not found:", missing_completed.stdout)
        self.assertEqual(invalid_completed.returncode, 2)
        self.assertIn("requires a git slop plan payload", invalid_completed.stdout)
        self.assertEqual(unknown_slice_completed.returncode, 2)
        self.assertIn("Unknown plan slice id", unknown_slice_completed.stdout)

    def test_refactor_preview_cli_requires_plan(self) -> None:
        completed = run_cli("refactor-preview")

        self.assertEqual(completed.returncode, 2)
        self.assertIn("usage:", completed.stderr)


if __name__ == "__main__":
    unittest.main()
