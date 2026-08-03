from __future__ import annotations

import json
import re
import shutil
import subprocess
import tomllib
from pathlib import Path

ROOT_AGENTS = {
    "dependency_patcher": ".codex/agents/dependency-patcher.toml",
    "docs_taxonomist": ".codex/agents/docs-taxonomist.toml",
    "governance_auditor": ".codex/agents/governance-auditor.toml",
    "merge_gatekeeper": ".codex/agents/merge-gatekeeper.toml",
    "release_publisher": ".codex/agents/release-publisher.toml",
}

INSTALLED_PLUGIN_NAME = "project-management-workflows"

CODEX_CONFIG_PATH = Path(".codex/config.toml")
CI_PROFILE_SANDBOX_MODES = {
    "ci_readonly": "read-only",
    "ci_mutation": "workspace-write",
    "ci_release": "workspace-write",
}
CODEX_PROFILE_COPY_COMMAND = 'cp .codex/*.config.toml "$RUNNER_TEMP/codex-home/"'


def _installed_skill(skill_name: str) -> str:
    return f"${INSTALLED_PLUGIN_NAME}:{skill_name}"


AGENT_SKILL_REFERENCES = {
    "dependency_patcher": {_installed_skill("dependency-remediation")},
    "docs_taxonomist": {_installed_skill("docs-taxonomy")},
    "governance_auditor": {
        _installed_skill("ensure-quarter-milestones"),
        _installed_skill("github-backlog-mutate"),
        _installed_skill("label-palette-design"),
    },
    "merge_gatekeeper": {_installed_skill("merge-on-green")},
    "release_publisher": {_installed_skill("release-publish")},
}

EXPECTED_PLUGIN_URL = "https://github.com/coreycoto/agent-plugins.git"
EXPECTED_PLUGIN_SHA = "03f3724e4ff41376b4f0d10d83c9ec335fcdac3d"
EXPECTED_MARKETPLACE_NAME = "agent-plugins-marketplace"
MARKETPLACE_SOURCE_MANIFEST = Path(".agents/plugins/marketplace-source.json")
GIT_SLOP_MARKETPLACE = Path(".agents/plugins/marketplace.json")
GIT_SLOP_PLUGIN_ROOT = Path("plugins/git-slop")
GIT_SLOP_MARKETPLACE_NAME = "git-slop-marketplace"
GIT_SLOP_PLUGIN_DOC_NAME = "`git-slop` Codex plugin"
GIT_SLOP_PLUGIN_NAME = "git-slop"
GIT_SLOP_PLUGIN_SKILLS = {
    "adopt-repo",
    "install-update",
    "interpret-results",
    "plan-maintenance",
    "run-report",
}
BOOTSTRAP_SCRIPT = Path("scripts/bootstrap_agent_plugins_marketplace.py")
PLUGIN_SKILL_CATALOG = {
    "dependency-remediation",
    "docs-taxonomy",
    "ensure-quarter-milestones",
    "github-backlog-mutate",
    "intake",
    "intake-preview",
    "label-palette-design",
    "merge-on-green",
    "plan-quarter-apply",
    "plan-quarter-preview",
    "plan-to-backlog-preview",
    "release-publish",
    "review-to-backlog-apply",
    "review-to-backlog-preview",
}
REMOVED_LOCAL_PLUGIN_ROOT = Path("plugins/project-management-workflows")
REMOVED_LOCAL_PLUGIN_REFERENCES = (
    "plugins/project-management-workflows/",
    "manage_home_local_plugin.py",
    "smoke_home_install.py",
)
REMOVED_TESTS = {
    "tests/test_github_surface_preflight.py",
    "tests/test_plugin_home_install.py",
}
WORKFLOW_ASSETS = {
    "dependency-remediation.yml": {
        "prompt": ".github/codex/prompts/dependency-remediation.md",
        "schema": ".github/codex/schemas/dependency-remediation.json",
        "skill": _installed_skill("dependency-remediation"),
        "agent_file": ".codex/agents/dependency-patcher.toml",
    },
    "docs-taxonomy.yml": {
        "prompt": ".github/codex/prompts/docs-taxonomy.md",
        "schema": ".github/codex/schemas/docs-taxonomy.json",
        "skill": _installed_skill("docs-taxonomy"),
        "agent_file": ".codex/agents/docs-taxonomist.toml",
    },
    "governance-reconcile.yml": {
        "prompt": ".github/codex/prompts/governance-reconcile.md",
        "schema": ".github/codex/schemas/governance-reconcile.json",
        "skill": _installed_skill("github-backlog-mutate"),
        "agent_file": ".codex/agents/governance-auditor.toml",
    },
    "merge-on-green.yml": {
        "prompt": ".github/codex/prompts/merge-on-green.md",
        "schema": ".github/codex/schemas/merge-on-green.json",
        "skill": _installed_skill("merge-on-green"),
        "agent_file": ".codex/agents/merge-gatekeeper.toml",
    },
    "release-publish.yml": {
        "prompt": ".github/codex/prompts/release-publish.md",
        "schema": ".github/codex/schemas/release-publish.json",
        "skill": _installed_skill("release-publish"),
        "agent_file": ".codex/agents/release-publisher.toml",
    },
}


def _load_toml(path: Path) -> dict[str, object]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _run_execpolicy(rule_path: Path, repo_root: Path) -> list[str]:
    if shutil.which("codex") is None:
        return []
    commands = [
        [
            "codex",
            "execpolicy",
            "check",
            "--rules",
            str(rule_path),
            "--",
            "git",
            "push",
            "origin",
            "main",
        ],
        [
            "codex",
            "execpolicy",
            "check",
            "--rules",
            str(rule_path),
            "--",
            "gh",
            "release",
            "create",
            "v1.2.3",
        ],
        [
            "codex",
            "execpolicy",
            "check",
            "--rules",
            str(rule_path),
            "--",
            "gh",
            "pr",
            "merge",
            "123",
            "--squash",
        ],
    ]
    errors: list[str] = []
    for command in commands:
        completed = subprocess.run(
            command,
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        if completed.returncode != 0:
            errors.append(
                "execpolicy check failed for "
                f"{' '.join(command[6:])}: "
                f"{completed.stderr.strip() or completed.stdout.strip()}"
            )
    return errors


def _is_sha(value: object) -> bool:
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{40}", value) is not None


def _validate_codex_config(repo_root: Path) -> list[str]:
    errors: list[str] = []
    config_path = repo_root / CODEX_CONFIG_PATH
    if not config_path.exists():
        errors.append(".codex/config.toml is missing.")
    else:
        payload = _load_toml(config_path)
        if payload.get("approval_policy") != "on-request":
            errors.append(".codex/config.toml must default approval_policy to on-request.")
        if payload.get("sandbox_mode") != "workspace-write":
            errors.append(".codex/config.toml must default sandbox_mode to workspace-write.")
        if "profile" in payload:
            errors.append(
                ".codex/config.toml must not define the legacy profile selector; "
                "select a standalone profile with --profile."
            )
        if "profiles" in payload:
            errors.append(
                ".codex/config.toml must not define legacy [profiles.*] tables; "
                "use standalone .codex/<profile>.config.toml files."
            )

    for profile_name, sandbox_mode in CI_PROFILE_SANDBOX_MODES.items():
        relative_path = Path(".codex") / f"{profile_name}.config.toml"
        profile_path = repo_root / relative_path
        if not profile_path.exists():
            errors.append(f"{relative_path.as_posix()} is missing.")
            continue
        payload = _load_toml(profile_path)
        if payload.get("approval_policy") != "never":
            errors.append(
                f"{relative_path.as_posix()} must set top-level approval_policy to never."
            )
        if payload.get("sandbox_mode") != sandbox_mode:
            errors.append(
                f"{relative_path.as_posix()} must set top-level sandbox_mode to {sandbox_mode}."
            )

    return errors


def validate_codex_surface(repo_root: Path, *, require_codex_cli: bool = False) -> list[str]:
    errors: list[str] = []

    for relative_path in (
        "AGENTS.md",
        ".codex/README.md",
        "config/github/README.md",
        "config/labels/README.md",
        ".agents/README.md",
    ):
        if not (repo_root / relative_path).exists():
            errors.append(f"{relative_path} is missing.")

    errors.extend(_validate_codex_config(repo_root))

    rule_path = repo_root / ".codex" / "rules" / "git.rules"
    if not rule_path.exists():
        errors.append(".codex/rules/git.rules is missing.")
    elif require_codex_cli and shutil.which("codex") is None:
        errors.append("codex CLI is required but not installed.")
    else:
        errors.extend(_run_execpolicy(rule_path, repo_root))

    marketplace_source_path = repo_root / MARKETPLACE_SOURCE_MANIFEST
    if not marketplace_source_path.exists():
        errors.append(".agents/plugins/marketplace-source.json is missing.")
    else:
        manifest = json.loads(marketplace_source_path.read_text(encoding="utf-8"))
        if manifest.get("marketplace_name") != EXPECTED_MARKETPLACE_NAME:
            errors.append(
                "Consumer bootstrap manifest must use the agent-plugins marketplace name."
            )
        if manifest.get("source_url") != EXPECTED_PLUGIN_URL:
            errors.append("Consumer bootstrap manifest must point at coreycoto/agent-plugins.git.")
        if not _is_sha(manifest.get("ref")):
            errors.append("Consumer bootstrap manifest must pin an immutable 40-character sha.")
        elif manifest.get("ref") != EXPECTED_PLUGIN_SHA:
            errors.append("Consumer bootstrap manifest must pin the expected agent-plugins commit.")
        if manifest.get("required_plugin") != INSTALLED_PLUGIN_NAME:
            errors.append(
                "Consumer bootstrap manifest must require the project-management-workflows plugin."
            )

    if not (repo_root / BOOTSTRAP_SCRIPT).exists():
        errors.append("scripts/bootstrap_agent_plugins_marketplace.py is missing.")

    marketplace_path = repo_root / GIT_SLOP_MARKETPLACE
    if not marketplace_path.exists():
        errors.append(".agents/plugins/marketplace.json is missing for git-slop marketplace.")
    else:
        marketplace = json.loads(marketplace_path.read_text(encoding="utf-8"))
        if marketplace.get("name") != GIT_SLOP_MARKETPLACE_NAME:
            errors.append(".agents/plugins/marketplace.json must define git-slop-marketplace.")
        plugins = marketplace.get("plugins")
        if not isinstance(plugins, list):
            errors.append(".agents/plugins/marketplace.json must define plugins list.")
        else:
            plugin_entries = [
                plugin
                for plugin in plugins
                if isinstance(plugin, dict) and plugin.get("name") == GIT_SLOP_PLUGIN_NAME
            ]
            if len(plugin_entries) != 1:
                errors.append(
                    ".agents/plugins/marketplace.json must define exactly one git-slop plugin."
                )
            else:
                source = plugin_entries[0].get("source")
                if not isinstance(source, dict) or source.get("path") != "./plugins/git-slop":
                    errors.append("git-slop marketplace entry must point at ./plugins/git-slop.")

    plugin_root = repo_root / GIT_SLOP_PLUGIN_ROOT
    plugin_manifest_path = plugin_root / ".codex-plugin" / "plugin.json"
    if not plugin_manifest_path.exists():
        errors.append("plugins/git-slop/.codex-plugin/plugin.json is missing.")
    else:
        plugin_manifest = json.loads(plugin_manifest_path.read_text(encoding="utf-8"))
        if plugin_manifest.get("name") != GIT_SLOP_PLUGIN_NAME:
            errors.append("git-slop plugin manifest must use name git-slop.")
        if plugin_manifest.get("skills") != "./skills/":
            errors.append("git-slop plugin manifest must expose ./skills/.")
    for skill_name in GIT_SLOP_PLUGIN_SKILLS:
        skill_path = plugin_root / "skills" / skill_name / "SKILL.md"
        if not skill_path.exists():
            errors.append(f"plugins/git-slop skill is missing: {skill_name}.")

    if (repo_root / REMOVED_LOCAL_PLUGIN_ROOT).exists():
        errors.append("Local plugins/project-management-workflows/ tree should have been removed.")

    for removed_test in REMOVED_TESTS:
        if (repo_root / removed_test).exists():
            errors.append(f"{removed_test} should have been removed from the consumer repo.")

    for agent_name, relative_path in ROOT_AGENTS.items():
        agent_path = repo_root / relative_path
        if not agent_path.exists():
            errors.append(f"Missing custom agent file: {relative_path}.")
            continue
        payload = agent_path.read_text(encoding="utf-8")
        if "[[skills.config]]" in payload:
            errors.append(f"{relative_path} must not bind local plugin paths.")
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES:
            if forbidden in payload:
                errors.append(f"{relative_path} must not reference {forbidden}.")
        for skill_name in AGENT_SKILL_REFERENCES[agent_name]:
            if skill_name not in payload:
                errors.append(f"{relative_path} must mention {skill_name}.")

    for workflow_name, assets in WORKFLOW_ASSETS.items():
        workflow_path = repo_root / ".github" / "workflows" / workflow_name
        prompt_path = repo_root / assets["prompt"]
        schema_path = repo_root / assets["schema"]
        if not workflow_path.exists():
            errors.append(f"Missing workflow file {workflow_name}.")
        if not prompt_path.exists():
            errors.append(f"Missing prompt file {assets['prompt']}.")
            continue
        if not schema_path.exists():
            errors.append(f"Missing schema file {assets['schema']}.")
        prompt_text = prompt_path.read_text(encoding="utf-8")
        if assets["skill"] not in prompt_text:
            errors.append(f"{assets['prompt']} must mention {assets['skill']}.")
        if assets["agent_file"] not in prompt_text:
            errors.append(f"{assets['prompt']} must mention {assets['agent_file']}.")
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES:
            if forbidden in prompt_text:
                errors.append(f"{assets['prompt']} must not reference {forbidden}.")

    for relative_path in (
        "AGENTS.md",
        ".agents/README.md",
        ".codex/README.md",
        "config/github/README.md",
        "config/labels/README.md",
    ):
        text = (repo_root / relative_path).read_text(encoding="utf-8")
        if EXPECTED_PLUGIN_URL not in text and "agent-plugins" not in text:
            errors.append(
                f"{relative_path} must point readers to the agent-plugins source of truth."
            )
        if relative_path in {"AGENTS.md", ".agents/README.md", ".codex/README.md"}:
            if "marketplace-source.json" not in text:
                errors.append(
                    f"{relative_path} must mention .agents/plugins/marketplace-source.json."
                )
            if GIT_SLOP_PLUGIN_DOC_NAME not in text:
                errors.append(f"{relative_path} must mention {GIT_SLOP_PLUGIN_DOC_NAME}.")
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES:
            if forbidden in text:
                errors.append(f"{relative_path} must not reference {forbidden}.")

    smoke_script = repo_root / "scripts" / "smoke_plugin_consumer.py"
    if not smoke_script.exists():
        errors.append("scripts/smoke_plugin_consumer.py is missing.")

    agent_plugin_workflows = (
        "ci.yml",
        "dependency-remediation.yml",
        "docs-taxonomy.yml",
        "governance-reconcile.yml",
        "merge-on-green.yml",
    )
    for workflow_name in agent_plugin_workflows:
        workflow_path = repo_root / ".github" / "workflows" / workflow_name
        if not workflow_path.exists():
            errors.append(f"Missing workflow file {workflow_name}.")
            continue
        workflow_text = workflow_path.read_text(encoding="utf-8")
        if "AGENT_PLUGINS_GIT_TOKEN" not in workflow_text:
            errors.append(f"{workflow_name} must configure AGENT_PLUGINS_GIT_TOKEN access.")
        if EXPECTED_PLUGIN_URL not in workflow_text:
            errors.append(f"{workflow_name} must reference the agent-plugins repo URL.")
        if (
            workflow_name != "ci.yml"
            and "scripts/bootstrap_agent_plugins_marketplace.py install" not in workflow_text
        ):
            errors.append(
                f"{workflow_name} must bootstrap the pinned agent-plugins marketplace source."
            )
        if workflow_name == "ci.yml" and "scripts/smoke_plugin_consumer.py" not in workflow_text:
            errors.append("ci.yml must run scripts/smoke_plugin_consumer.py.")
        if "openai/codex-action@v1" in workflow_text:
            if "$RUNNER_TEMP/codex-home" not in workflow_text:
                errors.append(f"{workflow_name} must prepare a temporary isolated Codex home.")
            if 'cp .codex/config.toml "$RUNNER_TEMP/codex-home/config.toml"' not in workflow_text:
                errors.append(
                    f"{workflow_name} must copy repo Codex config into the isolated Codex home."
                )
            if CODEX_PROFILE_COPY_COMMAND not in workflow_text:
                errors.append(
                    f"{workflow_name} must copy standalone Codex profiles into the isolated "
                    "Codex home."
                )
            if "codex-home: ${{ runner.temp }}/codex-home" not in workflow_text:
                errors.append(f"{workflow_name} must pass the isolated Codex home to codex-action.")

    release_workflow = repo_root / ".github" / "workflows" / "release-publish.yml"
    if not release_workflow.exists():
        errors.append("Missing workflow file release-publish.yml.")
    else:
        release_text = release_workflow.read_text(encoding="utf-8")
        if "AGENT_PLUGINS_GIT_TOKEN" in release_text:
            errors.append(
                "release-publish.yml must keep Rust artifact publication "
                "decoupled from agent-plugins credentials."
            )
        for required in (
            "cargo publish --dry-run --locked",
            "scripts/build_release_manifest.py",
            "dist/SHA256SUMS",
            "dist/release-manifest.json",
            "gh release upload",
        ):
            if required not in release_text:
                errors.append(f"release-publish.yml must include {required}.")

    return errors
