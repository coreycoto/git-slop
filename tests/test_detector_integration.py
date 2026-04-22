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

from git_slop import reporting  # noqa: E402
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
            self.assertEqual(report["schema_version"], 1)
            self.assertEqual(yaml_report["schema_version"], 1)
            self.assertEqual(report["repo"]["repo_name"], repo_root.name)
            self.assertIn("files", report)
            self.assertIn("folders", report)
            self.assertIn("action_queue", report)
            self.assertEqual(report["stats"]["analyzed_file_count"], 1)
            self.assertEqual(report["stats"]["skipped_ignored_count"], 1)
            self.assertEqual(report["files"][0]["path"], "README.md")
            self.assertEqual(report["folders"][0]["path"], ".")
            summary_contents = summary_md.read_text(encoding="utf-8")
            self.assertIn("Top Hotspots", summary_contents)
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

            json_show_completed = run_cli(repo_root, "show", "src", "--format", "json")
            self.assertEqual(json_show_completed.returncode, 0, json_show_completed.stderr)
            folder_record = json.loads(json_show_completed.stdout)
            self.assertEqual(folder_record["path"], "src")

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
