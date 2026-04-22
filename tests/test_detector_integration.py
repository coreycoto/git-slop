from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

REPO_ROOT = Path(__file__).resolve().parents[1]
SRC_DIR = REPO_ROOT / "src"
if str(SRC_DIR) not in sys.path:
    sys.path.insert(0, str(SRC_DIR))

from git_slop import history, reporting  # noqa: E402
from git_slop.detector import run_detector  # noqa: E402


def run_cli(repo_root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        str(SRC_DIR) if not existing_pythonpath else f"{SRC_DIR}{os.pathsep}{existing_pythonpath}"
    )
    return subprocess.run(
        [sys.executable, "-m", "git_slop", *args],
        cwd=repo_root,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )


def run_git(repo_root: Path, *args: str, env: dict[str, str] | None = None) -> None:
    subprocess.run(
        ["git", *args],
        cwd=repo_root,
        env=env,
        capture_output=True,
        text=True,
        check=True,
    )


def commit_all(repo_root: Path, *, message: str, timestamp: str) -> None:
    env = os.environ.copy()
    env["GIT_AUTHOR_DATE"] = timestamp
    env["GIT_COMMITTER_DATE"] = timestamp
    run_git(repo_root, "add", ".", env=env)
    run_git(repo_root, "commit", "-m", message, env=env)


def init_repo(repo_root: Path) -> None:
    run_git(repo_root, "init", "-b", "main")
    run_git(repo_root, "config", "user.name", "Git Slop Tests")
    run_git(repo_root, "config", "user.email", "git-slop-tests@example.com")


class DetectorIntegrationTests(unittest.TestCase):
    def test_find_generates_report_without_prior_init(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)
            (repo_root / "README.md").write_text("# Sample\n\nhello world\n", encoding="utf-8")
            (repo_root / "uv.lock").write_text(
                "package = [\n" + ("x = 1\n" * 5000) + "]\n",
                encoding="utf-8",
            )
            commit_all(repo_root, message="initial", timestamp="2025-01-01T00:00:00Z")

            completed = run_cli(repo_root, "find")

            self.assertEqual(completed.returncode, 0, completed.stderr)
            latest_root = repo_root / ".slop" / "latest"
            report_json = latest_root / "report.json"
            report_yaml = latest_root / "report.yaml"
            summary_md = latest_root / "summary.md"
            self.assertTrue(report_json.exists())
            self.assertTrue(report_yaml.exists())
            self.assertTrue(summary_md.exists())

            report = json.loads(report_json.read_text(encoding="utf-8"))
            yaml_report = yaml.safe_load(report_yaml.read_text(encoding="utf-8"))
            self.assertEqual(report["schema_version"], 2)
            self.assertEqual(yaml_report["schema_version"], 2)
            self.assertEqual(report["repo"]["repo_name"], repo_root.name)
            self.assertIn("files", report)
            self.assertIn("folders", report)
            self.assertIn("action_queue", report)
            self.assertIn("organization_metrics", report)
            self.assertIn("relationships", report)
            self.assertIn("clusters", report)
            self.assertEqual(report["organization_metrics"]["analysis_status"], "experimental")
            self.assertEqual(report["organization_metrics"]["analysis_version"], 1)
            self.assertEqual(report["stats"]["analyzed_file_count"], 1)
            self.assertEqual(report["stats"]["skipped_ignored_count"], 1)
            self.assertEqual(report["files"][0]["path"], "README.md")
            self.assertEqual(report["folders"][0]["path"], ".")
            summary_contents = summary_md.read_text(encoding="utf-8")
            self.assertIn("Top Hotspots", summary_contents)
            self.assertIn("Organization Health", summary_contents)
            self.assertIn("Next Action Queue", summary_contents)
            self.assertIn(
                (
                    "| Path | Priority | Context | Score | Tokens | Age | Revs | "
                    "Churn | Signal | Reasons |"
                ),
                summary_contents,
            )
            self.assertNotIn("uv.lock", [record["path"] for record in report["files"]])
            self.assertIn("README.md", completed.stdout)
            self.assertIn("Score", completed.stdout)
            self.assertIn("Organization Health", completed.stdout)

    def test_show_and_check_use_generated_report(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)
            app_path = repo_root / "src" / "app.py"
            app_path.parent.mkdir(parents=True, exist_ok=True)
            app_path.write_text("print('hello')\n", encoding="utf-8")
            commit_all(repo_root, message="initial", timestamp="2025-01-01T00:00:00Z")

            app_path.write_text("print('hello')\nprint('again')\n", encoding="utf-8")
            commit_all(repo_root, message="update", timestamp="2025-04-01T00:00:00Z")

            find_completed = run_cli(repo_root, "find")
            self.assertEqual(find_completed.returncode, 0, find_completed.stderr)

            show_completed = run_cli(repo_root, "show", "src/app.py")
            self.assertEqual(show_completed.returncode, 0, show_completed.stderr)
            self.assertIn("path: src/app.py", show_completed.stdout)
            self.assertIn("priority_band:", show_completed.stdout)
            self.assertIn("organization_health:", show_completed.stdout)
            self.assertIn("strongest_relationships:", show_completed.stdout)
            self.assertIn("cluster_memberships:", show_completed.stdout)

            json_show_completed = run_cli(repo_root, "show", "src", "--format", "json")
            self.assertEqual(json_show_completed.returncode, 0, json_show_completed.stderr)
            folder_record = json.loads(json_show_completed.stdout)
            self.assertEqual(folder_record["path"], "src")
            self.assertEqual(folder_record["record_type"], "folder")
            self.assertIn("organization_health", folder_record)

            pass_completed = run_cli(repo_root, "check")
            self.assertEqual(pass_completed.returncode, 0, pass_completed.stderr)
            self.assertIn("Check passed:", pass_completed.stdout)

            fail_completed = run_cli(repo_root, "check", "--fail-on-context-band", "compact")
            self.assertEqual(fail_completed.returncode, 1, fail_completed.stderr)
            self.assertIn("Check failed:", fail_completed.stdout)

    def test_find_can_follow_rename_history_for_age(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            legacy_path = repo_root / "src" / "legacy.py"
            legacy_path.parent.mkdir(parents=True, exist_ok=True)
            legacy_path.write_text("print('legacy')\n", encoding="utf-8")
            commit_all(repo_root, message="add legacy file", timestamp="2025-10-01T00:00:00Z")

            legacy_path.write_text("print('legacy')\nprint('still legacy')\n", encoding="utf-8")
            commit_all(repo_root, message="update legacy file", timestamp="2026-02-01T00:00:00Z")

            renamed_path = repo_root / "src" / "renamed.py"
            run_git(repo_root, "mv", "src/legacy.py", "src/renamed.py")
            commit_all(repo_root, message="rename legacy file", timestamp="2026-03-01T00:00:00Z")
            renamed_path.write_text(
                "print('legacy')\nprint('still legacy')\nprint('rename pass')\n",
                encoding="utf-8",
            )
            commit_all(repo_root, message="update renamed file", timestamp="2026-04-01T00:00:00Z")

            no_follow_find = run_cli(repo_root, "find")
            self.assertEqual(no_follow_find.returncode, 0, no_follow_find.stderr)
            no_follow_show = run_cli(repo_root, "show", "src/renamed.py", "--format", "json")
            self.assertEqual(no_follow_show.returncode, 0, no_follow_show.stderr)
            no_follow_record = json.loads(no_follow_show.stdout)
            self.assertGreater(no_follow_record["age_days"], 30)
            self.assertLess(no_follow_record["age_days"], 100)
            self.assertGreaterEqual(no_follow_record["revisions_window"], 1)

            slop_root = repo_root / ".slop"
            slop_root.mkdir(exist_ok=True)
            config_payload = {
                "schema_version": 1,
                "history": {"follow_renames": True},
            }
            (slop_root / "config.yaml").write_text(
                yaml.safe_dump(config_payload, sort_keys=False),
                encoding="utf-8",
            )

            follow_find = run_cli(repo_root, "find")
            self.assertEqual(follow_find.returncode, 0, follow_find.stderr)
            follow_show = run_cli(repo_root, "show", "src/renamed.py", "--format", "json")
            self.assertEqual(follow_show.returncode, 0, follow_show.stderr)
            follow_record = json.loads(follow_show.stdout)
            self.assertGreater(follow_record["age_days"], no_follow_record["age_days"])
            self.assertGreaterEqual(
                follow_record["revisions_window"],
                no_follow_record["revisions_window"],
            )

    def test_find_refreshes_latest_after_deleted_file_is_removed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            keep_path = repo_root / "keep.py"
            delete_path = repo_root / "delete_me.py"
            keep_path.write_text("print('keep')\n", encoding="utf-8")
            delete_path.write_text("print('delete')\n", encoding="utf-8")
            commit_all(repo_root, message="initial files", timestamp="2025-01-01T00:00:00Z")

            first_find = run_cli(repo_root, "find")
            self.assertEqual(first_find.returncode, 0, first_find.stderr)
            first_report_path = repo_root / ".slop" / "latest" / "report.json"
            self.assertIn(
                "delete_me.py",
                first_report_path.read_text(encoding="utf-8"),
            )

            delete_path.unlink()
            run_git(repo_root, "rm", "delete_me.py")
            commit_all(repo_root, message="remove deleted file", timestamp="2025-03-01T00:00:00Z")

            second_find = run_cli(repo_root, "find")
            self.assertEqual(second_find.returncode, 0, second_find.stderr)
            latest_root = repo_root / ".slop" / "latest"
            report_text = (latest_root / "report.json").read_text(encoding="utf-8")
            yaml_text = (latest_root / "report.yaml").read_text(encoding="utf-8")
            summary_text = (latest_root / "summary.md").read_text(encoding="utf-8")
            self.assertNotIn("delete_me.py", report_text)
            self.assertNotIn("delete_me.py", yaml_text)
            self.assertNotIn("delete_me.py", summary_text)
            self.assertIn("keep.py", report_text)

    def test_find_refreshes_latest_after_rename_to_current_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            legacy_path = repo_root / "src" / "legacy.py"
            legacy_path.parent.mkdir(parents=True, exist_ok=True)
            legacy_path.write_text("print('legacy')\n", encoding="utf-8")
            commit_all(repo_root, message="initial legacy file", timestamp="2025-01-01T00:00:00Z")

            first_find = run_cli(repo_root, "find")
            self.assertEqual(first_find.returncode, 0, first_find.stderr)
            self.assertIn(
                "src/legacy.py",
                (repo_root / ".slop" / "latest" / "report.json").read_text(encoding="utf-8"),
            )

            run_git(repo_root, "mv", "src/legacy.py", "src/renamed.py")
            commit_all(repo_root, message="rename legacy file", timestamp="2025-04-01T00:00:00Z")

            second_find = run_cli(repo_root, "find")
            self.assertEqual(second_find.returncode, 0, second_find.stderr)
            latest_root = repo_root / ".slop" / "latest"
            report_text = (latest_root / "report.json").read_text(encoding="utf-8")
            summary_text = (latest_root / "summary.md").read_text(encoding="utf-8")
            self.assertIn("src/renamed.py", report_text)
            self.assertNotIn("src/legacy.py", report_text)
            self.assertNotIn("src/legacy.py", summary_text)

    def test_failed_latest_bundle_write_preserves_previous_latest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            tracked_path = repo_root / "tracked.py"
            tracked_path.write_text("print('first')\n", encoding="utf-8")
            commit_all(repo_root, message="initial file", timestamp="2025-01-01T00:00:00Z")

            run_detector(repo_root, print_table=False)
            latest_root = repo_root / ".slop" / "latest"
            original_report = (latest_root / "report.json").read_text(encoding="utf-8")

            tracked_path.write_text("print('first')\nprint('second')\n", encoding="utf-8")
            commit_all(repo_root, message="update file", timestamp="2025-02-01T00:00:00Z")

            original_writer = reporting._write_bundle_files

            def flaky_writer(output_root: Path, bundle_payloads: dict[str, str]) -> None:
                if output_root.name.startswith(".latest-") and output_root.name.endswith(".tmp"):
                    raise OSError("simulated latest write failure")
                original_writer(output_root, bundle_payloads)

            with mock.patch("git_slop.reporting._write_bundle_files", side_effect=flaky_writer):
                with self.assertRaises(OSError):
                    run_detector(repo_root, print_table=False)

            self.assertEqual(
                (latest_root / "report.json").read_text(encoding="utf-8"),
                original_report,
            )
            leftover_paths = [
                path.name
                for path in latest_root.parent.iterdir()
                if path.name.startswith(".latest-")
            ]
            self.assertEqual(leftover_paths, [])

    def test_find_cold_and_warm_cache_runs_are_identical(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            (repo_root / "src").mkdir(parents=True, exist_ok=True)
            (repo_root / "src" / "alpha.py").write_text(
                "def alpha(value):\n    return value.strip().lower()\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "beta.py").write_text(
                "def beta(value):\n    return value.strip().lower()\n",
                encoding="utf-8",
            )
            commit_all(repo_root, message="initial", timestamp="2026-02-01T00:00:00Z")

            first_find = run_cli(repo_root, "find")
            self.assertEqual(first_find.returncode, 0, first_find.stderr)
            latest_root = repo_root / ".slop" / "latest"
            first_report = (latest_root / "report.json").read_text(encoding="utf-8")

            second_find = run_cli(repo_root, "find")
            self.assertEqual(second_find.returncode, 0, second_find.stderr)
            second_report = (latest_root / "report.json").read_text(encoding="utf-8")
            self.assertEqual(first_report, second_report)

            cache_root = repo_root / ".slop" / "cache" / "organization-health"
            cache_entries = list(cache_root.glob("*/*.json"))
            self.assertTrue(cache_entries)
            history_cache_root = repo_root / ".slop" / "cache" / "history"
            history_cache_entries = list(history_cache_root.glob("*/*.json"))
            self.assertTrue(history_cache_entries)

    def test_cache_key_changes_after_head_changes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            tracked_path = repo_root / "tracked.py"
            tracked_path.write_text("print('first')\n", encoding="utf-8")
            commit_all(repo_root, message="initial", timestamp="2026-02-01T00:00:00Z")

            first_find = run_cli(repo_root, "find")
            self.assertEqual(first_find.returncode, 0, first_find.stderr)

            tracked_path.write_text("print('first')\nprint('second')\n", encoding="utf-8")
            commit_all(repo_root, message="second", timestamp="2026-03-01T00:00:00Z")

            second_find = run_cli(repo_root, "find")
            self.assertEqual(second_find.returncode, 0, second_find.stderr)

            cache_root = repo_root / ".slop" / "cache" / "organization-health"
            cache_dirs = [path for path in cache_root.iterdir() if path.is_dir()]
            self.assertGreaterEqual(len(cache_dirs), 2)

    def test_history_cache_reuses_snapshot_on_warm_run(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            tracked_path = repo_root / "tracked.py"
            tracked_path.write_text("print('first')\n", encoding="utf-8")
            commit_all(repo_root, message="initial", timestamp="2026-02-01T00:00:00Z")

            run_detector(repo_root, print_table=False)

            with mock.patch(
                "git_slop.history._build_history_snapshot_uncached",
                side_effect=AssertionError("history snapshot should be loaded from cache"),
            ):
                run_detector(repo_root, print_table=False)

    def test_history_cache_key_changes_after_head_inventory_config_and_version_changes(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            tracked_path = repo_root / "tracked.py"
            tracked_path.write_text("print('first')\n", encoding="utf-8")
            commit_all(repo_root, message="initial", timestamp="2026-02-01T00:00:00Z")

            run_detector(repo_root, print_table=False)
            cache_root = repo_root / ".slop" / "cache" / "history"
            cache_dirs = [path for path in cache_root.iterdir() if path.is_dir()]
            self.assertEqual(len(cache_dirs), 1)

            tracked_path.write_text("print('first')\nprint('second')\n", encoding="utf-8")
            commit_all(repo_root, message="second", timestamp="2026-03-01T00:00:00Z")
            run_detector(repo_root, print_table=False)
            cache_dirs = [path for path in cache_root.iterdir() if path.is_dir()]
            self.assertEqual(len(cache_dirs), 2)

            config_root = repo_root / ".slop"
            config_root.mkdir(exist_ok=True)
            (config_root / "config.yaml").write_text(
                yaml.safe_dump(
                    {
                        "schema_version": 1,
                        "history": {"follow_renames": True},
                    },
                    sort_keys=False,
                ),
                encoding="utf-8",
            )
            run_detector(repo_root, print_table=False)
            cache_dirs = [path for path in cache_root.iterdir() if path.is_dir()]
            self.assertEqual(len(cache_dirs), 3)

            extra_path = repo_root / "extra.py"
            extra_path.write_text("print('extra')\n", encoding="utf-8")
            commit_all(repo_root, message="third", timestamp="2026-04-01T00:00:00Z")
            run_detector(repo_root, print_table=False)
            cache_dirs = [path for path in cache_root.iterdir() if path.is_dir()]
            self.assertEqual(len(cache_dirs), 4)

            patched_version = history.HISTORY_ANALYSIS_VERSION + 1
            with mock.patch.object(
                history,
                "HISTORY_ANALYSIS_VERSION",
                patched_version,
            ):
                run_detector(repo_root, print_table=False)
            cache_dirs = [path for path in cache_root.iterdir() if path.is_dir()]
            self.assertEqual(len(cache_dirs), 5)

    def test_find_emits_duplicate_coupling_and_cluster_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_repo(repo_root)

            alpha_path = repo_root / "src" / "alpha.py"
            beta_path = repo_root / "pkg" / "beta.py"
            gamma_path = repo_root / "docs" / "gamma.md"
            alpha_path.parent.mkdir(parents=True, exist_ok=True)
            beta_path.parent.mkdir(parents=True, exist_ok=True)
            gamma_path.parent.mkdir(parents=True, exist_ok=True)

            duplicate_block = "\n".join(
                [
                    "def shared_logic(value):",
                    "    normalized = value.strip().lower()",
                    "    tokens = normalized.split()",
                    "    return [token for token in tokens if token]",
                ]
                * 20
            )
            alpha_path.write_text(f"{duplicate_block}\n", encoding="utf-8")
            beta_path.write_text(f"{duplicate_block}\n", encoding="utf-8")
            gamma_path.write_text(
                "\n".join(
                    [
                        "shared logic converts values into normalized tokens",
                        "shared logic keeps token order stable across boundaries",
                    ]
                    * 30
                )
                + "\n",
                encoding="utf-8",
            )
            commit_all(repo_root, message="initial structure", timestamp="2025-12-01T00:00:00Z")

            for index, timestamp in enumerate(
                [
                    "2026-01-01T00:00:00Z",
                    "2026-02-01T00:00:00Z",
                    "2026-03-01T00:00:00Z",
                ],
                start=1,
            ):
                alpha_path.write_text(
                    f"{duplicate_block}\n# alpha change {index}\n",
                    encoding="utf-8",
                )
                beta_path.write_text(
                    f"{duplicate_block}\n# beta change {index}\n",
                    encoding="utf-8",
                )
                commit_all(repo_root, message=f"paired change {index}", timestamp=timestamp)

            for index, timestamp in enumerate(
                [
                    "2026-03-10T00:00:00Z",
                    "2026-03-20T00:00:00Z",
                    "2026-03-30T00:00:00Z",
                    "2026-04-05T00:00:00Z",
                    "2026-04-10T00:00:00Z",
                ],
                start=1,
            ):
                gamma_path.write_text(
                    (
                        "\n".join(
                            [
                                "shared logic converts values into normalized tokens",
                                "shared logic keeps token order stable across boundaries",
                            ]
                            * 30
                        )
                        + f"\nindependent gamma note {index}\n"
                    ),
                    encoding="utf-8",
                )
                commit_all(repo_root, message=f"gamma-only change {index}", timestamp=timestamp)

            completed = run_cli(repo_root, "find")
            self.assertEqual(completed.returncode, 0, completed.stderr)

            report = json.loads(
                (repo_root / ".slop" / "latest" / "report.json").read_text(encoding="utf-8")
            )
            duplicate_pairs = {
                (item["source_path"], item["target_path"])
                for item in report["relationships"]["duplicate_neighborhoods"]
            }
            self.assertIn(("pkg/beta.py", "src/alpha.py"), duplicate_pairs)

            coupling_pairs = {
                (item["source_path"], item["target_path"])
                for item in report["relationships"]["temporal_coupling_edges"]
            }
            self.assertIn(("pkg/beta.py", "src/alpha.py"), coupling_pairs)

            boundary_cluster_members = [
                set(cluster["member_paths"])
                for cluster in report["clusters"]["boundary_leakage_clusters"]
            ]
            self.assertTrue(
                any(
                    {"pkg/beta.py", "src/alpha.py"}.issubset(member_paths)
                    for member_paths in boundary_cluster_members
                )
            )

            overlay_by_path = {
                overlay["path"]: overlay for overlay in report["organization_metrics"]["files"]
            }
            self.assertGreater(overlay_by_path["src/alpha.py"]["duplication_pressure"], 0.0)
            self.assertGreater(overlay_by_path["src/alpha.py"]["coupling_pressure"], 0.0)
            self.assertGreater(
                overlay_by_path["src/alpha.py"]["cross_boundary_edge_count"],
                0,
            )
