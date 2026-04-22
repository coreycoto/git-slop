from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SRC_DIR = REPO_ROOT / "src"
sys.path.insert(0, str(SRC_DIR))

import git_slop  # noqa: E402


def run_cli(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        str(SRC_DIR) if not existing_pythonpath else f"{SRC_DIR}{os.pathsep}{existing_pythonpath}"
    )
    return subprocess.run(
        [sys.executable, "-m", "git_slop", *args],
        cwd=cwd or REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )


def init_git_repo(repo_root: Path) -> None:
    subprocess.run(
        ["git", "init", "-b", "main"], cwd=repo_root, check=True, capture_output=True, text=True
    )
    subprocess.run(
        ["git", "config", "user.name", "Git Slop Tests"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )
    subprocess.run(
        ["git", "config", "user.email", "git-slop-tests@example.com"],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
    )


class CliSmokeTests(unittest.TestCase):
    def test_package_import_exposes_version(self) -> None:
        self.assertEqual(git_slop.__version__, "0.1.3")

    def test_help_lists_registered_commands(self) -> None:
        completed = run_cli("--help")

        self.assertEqual(completed.returncode, 0)
        self.assertIn("init", completed.stdout)
        self.assertIn("find", completed.stdout)
        self.assertIn("show", completed.stdout)
        self.assertIn("check", completed.stdout)
        self.assertIn("version", completed.stdout)

    def test_version_command_works_via_python_module(self) -> None:
        completed = run_cli("version")

        self.assertEqual(completed.returncode, 0)
        self.assertEqual(completed.stdout.strip(), "git-slop 0.1.3")
        self.assertEqual(completed.stderr, "")

    def test_show_without_report_returns_usage_error(self) -> None:
        completed = run_cli("show", "README.md", "--report", "tmp/missing-report.json")

        self.assertEqual(completed.returncode, 2)
        self.assertIn("Report not found:", completed.stdout)
        self.assertEqual(completed.stderr, "")

    def test_check_without_report_returns_usage_error(self) -> None:
        completed = run_cli("check", "--report", "tmp/missing-report.json")

        self.assertEqual(completed.returncode, 2)
        self.assertIn("Report not found:", completed.stdout)
        self.assertEqual(completed.stderr, "")

    def test_init_writes_expected_state(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init_git_repo(repo_root)

            completed = run_cli("init", cwd=repo_root)

            self.assertEqual(completed.returncode, 0)
            self.assertTrue((repo_root / ".slop" / "config.yaml").exists())
            self.assertTrue((repo_root / ".slop" / ".gitignore").exists())
            self.assertTrue((repo_root / ".slop" / "latest").exists())
            self.assertTrue((repo_root / ".slop" / "runs").exists())
            self.assertTrue((repo_root / ".slop" / "cache").exists())
            self.assertIn("Initialized .slop/config.yaml", completed.stdout)


if __name__ == "__main__":
    unittest.main()
