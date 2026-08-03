from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from git_slop.integrations.agents.codex_surface import (
    AGENT_SKILL_REFERENCES,
    CI_PROFILE_SANDBOX_MODES,
    CODEX_CONFIG_PATH,
    CODEX_PROFILE_COPY_COMMAND,
    EXPECTED_MARKETPLACE_NAME,
    EXPECTED_PLUGIN_SHA,
    EXPECTED_PLUGIN_URL,
    GIT_SLOP_MARKETPLACE,
    GIT_SLOP_MARKETPLACE_NAME,
    GIT_SLOP_PLUGIN_ROOT,
    GIT_SLOP_PLUGIN_SKILLS,
    MARKETPLACE_SOURCE_MANIFEST,
    REMOVED_LOCAL_PLUGIN_ROOT,
    ROOT_AGENTS,
    WORKFLOW_ASSETS,
    _validate_codex_config,
    validate_codex_surface,
)

REPO_ROOT = Path(__file__).resolve().parents[1]


class CodexSurfaceTests(unittest.TestCase):
    def test_python_package_contains_only_maintainer_surfaces(self) -> None:
        package_root = REPO_ROOT / "src" / "git_slop"
        python_sources = {
            path.relative_to(package_root).as_posix() for path in package_root.rglob("*.py")
        }
        self.assertEqual(
            python_sources,
            {
                "__init__.py",
                "integrations/__init__.py",
                "integrations/agents/__init__.py",
                "integrations/agents/codex_surface.py",
            },
        )

        project = (REPO_ROOT / "pyproject.toml").read_text(encoding="utf-8")
        self.assertNotIn("tiktoken", project)

    def test_codex_surface_validation_passes(self) -> None:
        self.assertEqual(validate_codex_surface(REPO_ROOT), [])

    def test_expected_custom_agents_and_workflow_assets_exist(self) -> None:
        self.assertEqual(
            sorted(ROOT_AGENTS),
            [
                "dependency_patcher",
                "docs_taxonomist",
                "governance_auditor",
                "merge_gatekeeper",
                "release_publisher",
            ],
        )
        self.assertEqual(
            sorted(WORKFLOW_ASSETS),
            [
                "dependency-remediation.yml",
                "docs-taxonomy.yml",
                "governance-reconcile.yml",
                "merge-on-green.yml",
                "release-publish.yml",
            ],
        )

    def test_codex_config_uses_standalone_ci_profiles(self) -> None:
        base_config = (REPO_ROOT / CODEX_CONFIG_PATH).read_text(encoding="utf-8")
        self.assertNotIn("profile =", base_config)
        self.assertNotIn("[profiles", base_config)

        for profile_name, sandbox_mode in CI_PROFILE_SANDBOX_MODES.items():
            profile_path = REPO_ROOT / ".codex" / f"{profile_name}.config.toml"
            payload = profile_path.read_text(encoding="utf-8")
            self.assertIn('approval_policy = "never"', payload)
            self.assertIn(f'sandbox_mode = "{sandbox_mode}"', payload)

    def test_codex_config_validation_rejects_legacy_profiles(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            repo_root = Path(temp_dir)
            codex_root = repo_root / ".codex"
            codex_root.mkdir()
            (codex_root / "config.toml").write_text(
                'approval_policy = "on-request"\n'
                'sandbox_mode = "workspace-write"\n'
                'profile = "ci_mutation"\n'
                "[profiles.ci_mutation]\n"
                'approval_policy = "never"\n',
                encoding="utf-8",
            )
            for profile_name, sandbox_mode in CI_PROFILE_SANDBOX_MODES.items():
                (codex_root / f"{profile_name}.config.toml").write_text(
                    f'approval_policy = "never"\nsandbox_mode = "{sandbox_mode}"\n',
                    encoding="utf-8",
                )

            errors = _validate_codex_config(repo_root)

        self.assertIn(
            ".codex/config.toml must not define the legacy profile selector; "
            "select a standalone profile with --profile.",
            errors,
        )
        self.assertIn(
            ".codex/config.toml must not define legacy [profiles.*] tables; "
            "use standalone .codex/<profile>.config.toml files.",
            errors,
        )

    def test_codex_config_validation_requires_safe_standalone_profiles(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            repo_root = Path(temp_dir)
            codex_root = repo_root / ".codex"
            codex_root.mkdir()
            (codex_root / "config.toml").write_text(
                'approval_policy = "on-request"\nsandbox_mode = "workspace-write"\n',
                encoding="utf-8",
            )
            (codex_root / "ci_readonly.config.toml").write_text(
                'approval_policy = "on-request"\nsandbox_mode = "workspace-write"\n',
                encoding="utf-8",
            )
            (codex_root / "ci_mutation.config.toml").write_text(
                'approval_policy = "never"\nsandbox_mode = "workspace-write"\n',
                encoding="utf-8",
            )

            errors = _validate_codex_config(repo_root)

        self.assertIn(
            ".codex/ci_readonly.config.toml must set top-level approval_policy to never.",
            errors,
        )
        self.assertIn(
            ".codex/ci_readonly.config.toml must set top-level sandbox_mode to read-only.",
            errors,
        )
        self.assertIn(".codex/ci_release.config.toml is missing.", errors)

    def test_codex_action_workflows_copy_standalone_profiles(self) -> None:
        for workflow_name in (
            "dependency-remediation.yml",
            "docs-taxonomy.yml",
            "governance-reconcile.yml",
            "merge-on-green.yml",
        ):
            workflow_text = (REPO_ROOT / ".github" / "workflows" / workflow_name).read_text(
                encoding="utf-8"
            )
            self.assertIn(CODEX_PROFILE_COPY_COMMAND, workflow_text)

    def test_custom_agents_reference_installed_skill_names_only(self) -> None:
        for agent_name, relative_path in ROOT_AGENTS.items():
            payload = (REPO_ROOT / relative_path).read_text(encoding="utf-8")
            self.assertNotIn("[[skills.config]]", payload)
            self.assertNotIn("plugins/project-management-workflows/", payload)
            for expected_skill in AGENT_SKILL_REFERENCES[agent_name]:
                self.assertIn(expected_skill, payload)

    def test_marketplace_source_manifest_pins_publisher_sha(self) -> None:
        manifest = json.loads((REPO_ROOT / MARKETPLACE_SOURCE_MANIFEST).read_text(encoding="utf-8"))
        self.assertEqual(manifest["marketplace_name"], EXPECTED_MARKETPLACE_NAME)
        self.assertEqual(manifest["source_url"], EXPECTED_PLUGIN_URL)
        self.assertEqual(manifest["ref"], EXPECTED_PLUGIN_SHA)
        self.assertEqual(manifest["required_plugin"], "project-management-workflows")

    def test_git_slop_marketplace_publishes_product_plugin(self) -> None:
        marketplace = json.loads((REPO_ROOT / GIT_SLOP_MARKETPLACE).read_text(encoding="utf-8"))
        self.assertEqual(marketplace["name"], GIT_SLOP_MARKETPLACE_NAME)
        self.assertEqual(
            marketplace["plugins"],
            [
                {
                    "name": "git-slop",
                    "source": {"source": "local", "path": "./plugins/git-slop"},
                    "policy": {
                        "installation": "AVAILABLE",
                        "authentication": "ON_INSTALL",
                    },
                    "category": "Developer Tools",
                }
            ],
        )

    def test_git_slop_plugin_exposes_expected_skills(self) -> None:
        plugin_root = REPO_ROOT / GIT_SLOP_PLUGIN_ROOT
        manifest = json.loads(
            (plugin_root / ".codex-plugin" / "plugin.json").read_text(encoding="utf-8")
        )
        self.assertEqual(manifest["name"], "git-slop")
        self.assertEqual(manifest["version"], "0.2.1")
        self.assertEqual(manifest["skills"], "./skills/")
        skill_dirs = {path.parent.name for path in (plugin_root / "skills").glob("*/SKILL.md")}
        self.assertEqual(skill_dirs, GIT_SLOP_PLUGIN_SKILLS)

    def test_git_slop_plugin_documents_health_command_contract(self) -> None:
        plugin_root = REPO_ROOT / GIT_SLOP_PLUGIN_ROOT
        run_report = (plugin_root / "skills" / "run-report" / "SKILL.md").read_text(
            encoding="utf-8"
        )
        health_reference = (
            plugin_root / "skills" / "run-report" / "references" / "health.md"
        ).read_text(encoding="utf-8")
        adopt_repo = (plugin_root / "skills" / "adopt-repo" / "SKILL.md").read_text(
            encoding="utf-8"
        )
        interpret_results = (plugin_root / "skills" / "interpret-results" / "SKILL.md").read_text(
            encoding="utf-8"
        )
        run_report_contract = " ".join(run_report.split())
        health_contract = " ".join(health_reference.split())
        adoption_contract = " ".join(adopt_repo.split())
        interpretation_contract = " ".join(interpret_results.split())

        for expected in (
            "git-slop health --report <report.json>",
            "git-slop health --format json",
            "writes its selected rendering to stdout",
            "does not rewrite `health.md`",
            "Use `check`",
            "references/health.md",
        ):
            self.assertIn(expected, run_report_contract)

        for expected in (
            "Every format writes to stdout",
            "does not rewrite that file",
            "Exit `0` means the selected report rendered successfully",
            "run `git-slop find` exactly once",
            "git-slop health --report path/to/report.json --format json",
            "git-slop health --report",
            "git-slop check --report",
            "does not modify report artifacts",
        ):
            self.assertIn(expected, health_contract)

        self.assertIn("actions/checkout@v7", adoption_contract)
        self.assertNotIn("actions/checkout@v6", adoption_contract)
        self.assertIn("run `find` once", adoption_contract)
        self.assertIn("Treat health output as advisory", interpretation_contract)

    def test_public_docs_explain_health_rendering_and_enforcement(self) -> None:
        command_guide = (REPO_ROOT / "docs" / "commands.md").read_text(encoding="utf-8")
        report_contract = (REPO_ROOT / "docs" / "report-contract.md").read_text(encoding="utf-8")
        action_guide = (REPO_ROOT / "docs" / "github-action.md").read_text(encoding="utf-8")
        command_contract = " ".join(command_guide.split())
        report_schema_contract = " ".join(report_contract.split())
        action_contract = " ".join(action_guide.split())

        for expected in (
            "Every format writes to standard output",
            "never rewrites `.slop/latest/health.md`",
            "successful rendering exits 0",
            "Use `git-slop check`",
            "# Repository Health",
            "git-slop explain --path src/parser.rs",
        ):
            self.assertIn(expected, command_contract)

        for expected in (
            "All three `health` formats write to standard output",
            "do not rewrite `.slop/latest/health.md`",
            "health.data_context_min_bytes",
            "health.folder_bands.refactor_required_max_direct_files",
            "health.summary_top_folders",
        ):
            self.assertIn(expected, report_schema_contract)

        self.assertIn("Run `git-slop find` once", action_contract)
        self.assertIn("git-slop health --report", action_contract)
        self.assertIn("`health` render exits 0", action_contract)

    def test_local_plugin_tree_is_removed(self) -> None:
        self.assertFalse((REPO_ROOT / REMOVED_LOCAL_PLUGIN_ROOT).exists())


if __name__ == "__main__":
    unittest.main()
