from __future__ import annotations

import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = REPO_ROOT / ".github" / "workflows"


class WorkflowContractTests(unittest.TestCase):
    def test_codex_workflows_materialize_standalone_profiles(self) -> None:
        for workflow_name in (
            "dependency-remediation.yml",
            "docs-taxonomy.yml",
            "governance-reconcile.yml",
            "merge-on-green.yml",
        ):
            workflow = (WORKFLOWS / workflow_name).read_text(encoding="utf-8")

            with self.subTest(workflow=workflow_name):
                self.assertIn(
                    'cp .codex/config.toml "$RUNNER_TEMP/codex-home/config.toml"',
                    workflow,
                )
                self.assertIn(
                    'cp .codex/*.config.toml "$RUNNER_TEMP/codex-home/"',
                    workflow,
                )
                self.assertIn('"--profile","ci_mutation"', workflow)

    def test_github_artifact_actions_use_node_24_runtime(self) -> None:
        action_surfaces = [REPO_ROOT / "action.yml", *sorted(WORKFLOWS.glob("*.yml"))]

        for path in action_surfaces:
            contents = path.read_text(encoding="utf-8")
            with self.subTest(path=path.relative_to(REPO_ROOT)):
                self.assertNotIn("actions/upload-artifact@v4", contents)
                self.assertNotIn("actions/upload-artifact@v5", contents)

    def test_dogfood_uses_rust_and_keeps_summary_artifact_bounded(self) -> None:
        workflow = (WORKFLOWS / "dogfood.yml").read_text(encoding="utf-8")

        self.assertIn("cargo build --release --locked", workflow)
        self.assertIn("target/release/git-slop find", workflow)
        self.assertIn("cat .slop/latest/health.md", workflow)
        self.assertIn("path: .slop/latest/health.md", workflow)
        self.assertNotIn("path: .slop/latest\n", workflow)
        self.assertIn("retention-days: 14", workflow)
        self.assertNotIn("uv run git-slop", workflow)

    def test_ci_runs_rust_quality_package_and_platform_smokes(self) -> None:
        workflow = (WORKFLOWS / "ci.yml").read_text(encoding="utf-8")

        self.assertIn("cargo fmt --all -- --check", workflow)
        self.assertIn("cargo clippy --all-targets --all-features --locked", workflow)
        self.assertIn("cargo test --all-targets --all-features --locked", workflow)
        self.assertIn("cargo package --locked", workflow)
        self.assertIn("cargo publish --dry-run --locked", workflow)
        self.assertIn("node --test action/*.test.mjs", workflow)
        self.assertIn("maintainer-compatibility:", workflow)
        self.assertIn("uv sync --group dev", workflow)
        self.assertIn("uv run python -m compileall src tests scripts", workflow)
        self.assertIn("uv run ruff check", workflow)
        self.assertIn("uv run pytest", workflow)
        self.assertIn("ubuntu-24.04", workflow)
        self.assertIn("macos-15", workflow)
        self.assertIn("windows-2025", workflow)
        self.assertIn("windows-11-arm", workflow)
        self.assertNotIn("macos-15-intel", workflow)
        self.assertNotIn("uv build", workflow)

    def test_release_builds_and_publishes_every_supported_archive(self) -> None:
        workflow = (WORKFLOWS / "release-publish.yml").read_text(encoding="utf-8")

        for target in (
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
            "aarch64-pc-windows-msvc",
        ):
            self.assertIn(target, workflow)
        self.assertNotIn("x86_64-apple-darwin", workflow)
        self.assertIn("git-slop-${RELEASE_TAG}-${TARGET}.tar.gz", workflow)
        self.assertIn("git-slop-${env:RELEASE_TAG}-${env:TARGET}.zip", workflow)
        self.assertIn("os: ubuntu-22.04-arm", workflow)
        self.assertIn(
            "          - os: windows-11-arm\n"
            "            target: aarch64-pc-windows-msvc\n"
            "            archive: zip",
            workflow,
        )
        self.assertNotIn("os: macos-15-intel", workflow)
        self.assertNotIn("os: ubuntu-24.04-arm", workflow)
        self.assertIn("dist/SHA256SUMS", workflow)
        self.assertIn("dist/release-manifest.json", workflow)
        self.assertIn("gh release upload", workflow)
        self.assertIn("cargo publish --dry-run --locked", workflow)
        self.assertIn("node --test action/*.test.mjs", workflow)
        self.assertNotIn("cargo publish --locked", workflow)
        self.assertNotIn("uv build", workflow)


if __name__ == "__main__":
    unittest.main()
