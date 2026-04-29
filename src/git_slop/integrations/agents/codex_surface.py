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
EXPECTED_PLUGIN_SHA = "1cb87285df878822bcbb561bc684a57a24362a37"
EXPECTED_MARKETPLACE_NAME = "agent-plugins-marketplace"
MARKETPLACE_SOURCE_MANIFEST = Path(".agents/plugins/marketplace-source.json")
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
REMOVED_CONSUMER_MARKETPLACE = Path(".agents/plugins/marketplace.json")
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

    config_path = repo_root / ".codex" / "config.toml"
    if not config_path.exists():
        errors.append(".codex/config.toml is missing.")
    else:
        payload = _load_toml(config_path)
        if payload.get("approval_policy") != "on-request":
            errors.append(".codex/config.toml must default approval_policy to on-request.")
        if payload.get("sandbox_mode") != "workspace-write":
            errors.append(".codex/config.toml must default sandbox_mode to workspace-write.")
        profiles = payload.get("profiles")
        if not isinstance(profiles, dict):
            errors.append(".codex/config.toml must define profiles.")
        else:
            for profile_name in ("ci_readonly", "ci_mutation", "ci_release"):
                profile = profiles.get(profile_name)
                if not isinstance(profile, dict):
                    errors.append(f".codex/config.toml missing profile {profile_name}.")
                    continue
                if profile.get("approval_policy") != "never":
                    errors.append(
                        ".codex/config.toml profile "
                        f"{profile_name} must set approval_policy to never."
                    )

    rule_path = repo_root / ".codex" / "rules" / "git.rules"
    if not rule_path.exists():
        errors.append(".codex/rules/git.rules is missing.")
    elif require_codex_cli and shutil.which("codex") is None:
        errors.append("codex CLI is required but not installed.")
    else:
        errors.extend(_run_execpolicy(rule_path, repo_root))

    if (repo_root / REMOVED_CONSUMER_MARKETPLACE).exists():
        errors.append(".agents/plugins/marketplace.json should not exist in consumer repos.")
    marketplace_source_path = repo_root / MARKETPLACE_SOURCE_MANIFEST
    if not marketplace_source_path.exists():
        errors.append(".agents/plugins/marketplace-source.json is missing.")
    else:
        manifest = json.loads(marketplace_source_path.read_text(encoding="utf-8"))
        if manifest.get("marketplace_name") != EXPECTED_MARKETPLACE_NAME:
            errors.append(
                "Consumer bootstrap manifest must use the "
                "agent-plugins marketplace name."
            )
        if manifest.get("source_url") != EXPECTED_PLUGIN_URL:
            errors.append("Consumer bootstrap manifest must point at coreycoto/agent-plugins.git.")
        if not _is_sha(manifest.get("ref")):
            errors.append("Consumer bootstrap manifest must pin an immutable 40-character sha.")
        elif manifest.get("ref") != EXPECTED_PLUGIN_SHA:
            errors.append(
                "Consumer bootstrap manifest must pin the expected "
                "agent-plugins commit."
            )
        if manifest.get("required_plugin") != INSTALLED_PLUGIN_NAME:
            errors.append(
                "Consumer bootstrap manifest must require the "
                "project-management-workflows plugin."
            )

    if not (repo_root / BOOTSTRAP_SCRIPT).exists():
        errors.append("scripts/bootstrap_agent_plugins_marketplace.py is missing.")

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
        if "agent-tools" in prompt_text or "agent_tools" in prompt_text:
            errors.append(
                f"{assets['prompt']} must use agent_plugins runtime APIs, not agent-tools."
            )

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
                f"{relative_path} must point readers to the agent-plugins "
                "source of truth."
            )
        if relative_path in {"AGENTS.md", ".agents/README.md", ".codex/README.md"}:
            if "marketplace-source.json" not in text:
                errors.append(
                    f"{relative_path} must mention .agents/plugins/marketplace-source.json."
                )
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES:
            if forbidden in text:
                errors.append(f"{relative_path} must not reference {forbidden}.")

    smoke_script = repo_root / "scripts" / "smoke_plugin_consumer.py"
    if not smoke_script.exists():
        errors.append("scripts/smoke_plugin_consumer.py is missing.")

    ci_workflows = (
        "ci.yml",
        "dependency-remediation.yml",
        "docs-taxonomy.yml",
        "governance-reconcile.yml",
        "merge-on-green.yml",
        "release-publish.yml",
    )
    for workflow_name in ci_workflows:
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
        if "AGENT_TOOLS_READ_TOKEN" in workflow_text or "agent-tools.git" in workflow_text:
            errors.append(f"{workflow_name} must not configure agent-tools dependency access.")
        if workflow_name == "ci.yml" and "scripts/smoke_plugin_consumer.py" not in workflow_text:
            errors.append("ci.yml must run scripts/smoke_plugin_consumer.py.")

    return errors
