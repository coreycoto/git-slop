from __future__ import annotations

import json
import shutil
import subprocess
import tomllib
from pathlib import Path

import yaml

from .skills import SKILL_SPECS

ROOT_AGENTS = {
    "dependency_patcher": ".codex/agents/dependency-patcher.toml",
    "merge_gatekeeper": ".codex/agents/merge-gatekeeper.toml",
    "governance_auditor": ".codex/agents/governance-auditor.toml",
    "docs_taxonomist": ".codex/agents/docs-taxonomist.toml",
    "release_publisher": ".codex/agents/release-publisher.toml",
}

AGENT_SKILL_BINDINGS = {
    "dependency_patcher": {
        "plugins/project-management-workflows/skills/dependency-remediation/SKILL.md",
    },
    "merge_gatekeeper": {
        "plugins/project-management-workflows/skills/merge-on-green/SKILL.md",
    },
    "governance_auditor": {
        "plugins/project-management-workflows/skills/github-backlog-mutate/SKILL.md",
        "plugins/project-management-workflows/skills/label-palette-design/SKILL.md",
        "plugins/project-management-workflows/skills/ensure-quarter-milestones/SKILL.md",
    },
    "docs_taxonomist": {
        "plugins/project-management-workflows/skills/docs-taxonomy/SKILL.md",
    },
    "release_publisher": {
        "plugins/project-management-workflows/skills/release-publish/SKILL.md",
    },
}

PLUGIN_SKILLS_ROOT = Path("plugins/project-management-workflows/skills")
PLUGIN_ROOT = Path("plugins/project-management-workflows")
PLUGIN_MANIFEST = PLUGIN_ROOT / ".codex-plugin" / "plugin.json"
PLUGIN_APP_MAPPING = PLUGIN_ROOT / ".app.json"
GITHUB_PREREQUISITE_REFERENCE = (
    "plugins/project-management-workflows/skills/_shared/references/"
    "github-runtime-prerequisites.md"
)
GITHUB_PREREQUISITE_REFERENCE_SKILL_PATH = (
    "../_shared/references/github-runtime-prerequisites.md"
)
GITHUB_PREFLIGHT_SCRIPT = (
    "plugins/project-management-workflows/scripts/preflight_github_surface.py"
)
HOME_LOCAL_INSTALL_HELPER = (
    "plugins/project-management-workflows/scripts/manage_home_local_plugin.py"
)
HOME_LOCAL_SMOKE_SCRIPT = (
    "plugins/project-management-workflows/scripts/smoke_home_install.py"
)
EXPECTED_GITHUB_CONNECTOR_ID = "connector_76869538009648d5b282a4bb21c3d157"
SKILL_RUNTIME_CLASSIFICATIONS = {
    "dependency-remediation": "github_required",
    "docs-taxonomy": "local_first",
    "ensure-quarter-milestones": "github_required",
    "github-backlog-mutate": "github_required",
    "intake": "github_required",
    "intake-preview": "github_required",
    "label-palette-design": "github_required",
    "merge-on-green": "github_required",
    "plan-quarter-apply": "github_required",
    "plan-quarter-preview": "github_required",
    "plan-to-backlog-preview": "github_required",
    "release-publish": "github_required",
    "review-to-backlog-apply": "github_required",
    "review-to-backlog-preview": "github_required",
}
LOCAL_FIRST_SKILLS = {
    skill_name
    for skill_name, classification in SKILL_RUNTIME_CLASSIFICATIONS.items()
    if classification == "local_first"
}
ALLOWED_SKILL_RUNTIME_CLASSIFICATIONS = {
    "github_required",
    "github_optional",
    "local_first",
}

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

WORKFLOW_ASSETS = {
    "dependency-remediation.yml": {
        "prompt": ".github/codex/prompts/dependency-remediation.md",
        "schema": ".github/codex/schemas/dependency-remediation.json",
    },
    "docs-taxonomy.yml": {
        "prompt": ".github/codex/prompts/docs-taxonomy.md",
        "schema": ".github/codex/schemas/docs-taxonomy.json",
    },
    "governance-reconcile.yml": {
        "prompt": ".github/codex/prompts/governance-reconcile.md",
        "schema": ".github/codex/schemas/governance-reconcile.json",
    },
    "merge-on-green.yml": {
        "prompt": ".github/codex/prompts/merge-on-green.md",
        "schema": ".github/codex/schemas/merge-on-green.json",
    },
    "release-publish.yml": {
        "prompt": ".github/codex/prompts/release-publish.md",
        "schema": ".github/codex/schemas/release-publish.json",
    },
}

REQUIRED_RUNTIME_DOCS = {
    ".codex/README.md",
    "config/github/README.md",
    "config/labels/README.md",
}

REMOVED_DOCS_PREFIX = "/".join(("docs", "engineering")) + "/"

PLUGIN_SHARED_REFERENCES = {
    "plugins/project-management-workflows/skills/_shared/references/agent-decision-patterns.md",
    "plugins/project-management-workflows/skills/_shared/references/agent-decision-rubric.md",
    "plugins/project-management-workflows/skills/_shared/references/backlog-project-contract.md",
    GITHUB_PREREQUISITE_REFERENCE,
    "plugins/project-management-workflows/skills/_shared/references/github-mutation-contract.md",
    "plugins/project-management-workflows/skills/_shared/references/label-palette-contract.md",
    "plugins/project-management-workflows/skills/_shared/references/review-triage.md",
    "plugins/project-management-workflows/skills/_shared/references/workflow-tooling-surface.md",
}


def _load_toml(path: Path) -> dict[str, object]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _load_yaml(path: Path) -> dict[str, object]:
    payload = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"{path} must contain a YAML mapping.")
    return payload


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


def validate_codex_surface(repo_root: Path, *, require_codex_cli: bool = False) -> list[str]:
    errors: list[str] = []

    agents_md = repo_root / "AGENTS.md"
    if not agents_md.exists():
        errors.append("AGENTS.md is missing.")

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
                    errors.append(f".codex/config.toml is missing profile '{profile_name}'.")
                    continue
                if profile.get("approval_policy") != "never":
                    errors.append(f"profile '{profile_name}' must use approval_policy = never.")

    rule_path = repo_root / ".codex" / "rules" / "git.rules"
    if not rule_path.exists():
        errors.append(".codex/rules/git.rules is missing.")
    elif require_codex_cli or shutil.which("codex") is not None:
        errors.extend(_run_execpolicy(rule_path, repo_root))
    elif require_codex_cli:
        errors.append("codex CLI is required to validate execpolicy rules but was not found.")

    for agent_name, relative_path in ROOT_AGENTS.items():
        agent_path = repo_root / relative_path
        if not agent_path.exists():
            errors.append(f"Custom agent '{agent_name}' is missing: {relative_path}")
            continue
        payload = _load_toml(agent_path)
        if payload.get("name") != agent_name:
            errors.append(f"{relative_path} must declare name = {agent_name!r}.")
        if "description" not in payload or "developer_instructions" not in payload:
            errors.append(f"{relative_path} must declare description and developer_instructions.")
        skills_config = payload.get("skills", {}).get("config", [])
        if not isinstance(skills_config, list) or not skills_config:
            errors.append(f"{relative_path} must declare at least one skills.config entry.")
            continue
        configured_skill_paths: set[str] = set()
        for skill_config in skills_config:
            if not isinstance(skill_config, dict):
                errors.append(f"{relative_path} contains an invalid skills.config entry.")
                continue
            configured_path = skill_config.get("path")
            if not isinstance(configured_path, str):
                errors.append(f"{relative_path} skills.config entries must declare a path.")
                continue
            resolved_path = (agent_path.parent / configured_path).resolve()
            try:
                relative_skill_path = resolved_path.relative_to(repo_root)
            except ValueError:
                errors.append(
                    f"{relative_path} skills.config path escapes the repository root: "
                    f"{configured_path}"
                )
                continue
            if not resolved_path.exists():
                errors.append(
                    f"{relative_path} references a missing skill dependency: "
                    f"{relative_skill_path}"
                )
                continue
            if not str(relative_skill_path).startswith(f"{PLUGIN_SKILLS_ROOT}/"):
                errors.append(
                    f"{relative_path} must reference plugin-owned skills; "
                    f"found {relative_skill_path}"
                )
                continue
            configured_skill_paths.add(str(relative_skill_path))
        expected_skills = AGENT_SKILL_BINDINGS[agent_name]
        if configured_skill_paths != expected_skills:
            errors.append(
                f"{relative_path} must bind exactly these plugin skills: "
                f"{sorted(expected_skills)}"
            )

    marketplace_path = repo_root / ".agents" / "plugins" / "marketplace.json"
    if not marketplace_path.exists():
        errors.append(".agents/plugins/marketplace.json is missing.")
    else:
        payload = json.loads(marketplace_path.read_text(encoding="utf-8"))
        plugins = payload.get("plugins")
        if not isinstance(plugins, list):
            errors.append(".agents/plugins/marketplace.json must contain a plugins array.")
        else:
            names = {item.get("name") for item in plugins if isinstance(item, dict)}
            if "project-management-workflows" not in names:
                errors.append("Marketplace must expose the project-management-workflows plugin.")
            else:
                for item in plugins:
                    if not isinstance(item, dict):
                        continue
                    if item.get("name") != "project-management-workflows":
                        continue
                    policy = item.get("policy")
                    installation = policy.get("installation") if isinstance(policy, dict) else None
                    if installation != "INSTALLED_BY_DEFAULT":
                        errors.append(
                            "Marketplace must install the project-management-workflows plugin "
                            "by default."
                        )
                    break

    plugin_manifest = repo_root / PLUGIN_MANIFEST
    if not plugin_manifest.exists():
        errors.append("Repo-local plugin manifest is missing.")
    else:
        payload = json.loads(plugin_manifest.read_text(encoding="utf-8"))
        if "[TODO:" in json.dumps(payload):
            errors.append("Repo-local plugin manifest still contains TODO placeholders.")
        skills_path = payload.get("skills")
        if skills_path != "./skills/":
            errors.append("Repo-local plugin manifest must expose skills at ./skills/.")
        apps_path = payload.get("apps")
        if apps_path != "./.app.json":
            errors.append("Repo-local plugin manifest must expose apps at ./.app.json.")

    plugin_app_mapping = repo_root / PLUGIN_APP_MAPPING
    if not plugin_app_mapping.exists():
        errors.append("Repo-local plugin .app.json is missing.")
    else:
        try:
            app_payload = json.loads(plugin_app_mapping.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            errors.append(f"{PLUGIN_APP_MAPPING} must contain valid JSON: {exc}")
        else:
            apps = app_payload.get("apps")
            if not isinstance(apps, dict):
                errors.append(f"{PLUGIN_APP_MAPPING} must define an apps mapping.")
            else:
                github_app = apps.get("github")
                if not isinstance(github_app, dict):
                    errors.append(f"{PLUGIN_APP_MAPPING} must define apps.github.")
                else:
                    connector_id = github_app.get("id")
                    if connector_id != EXPECTED_GITHUB_CONNECTOR_ID:
                        errors.append(
                            f"{PLUGIN_APP_MAPPING} must map apps.github.id to "
                            f"{EXPECTED_GITHUB_CONNECTOR_ID}."
                        )

    for relative_path in sorted(PLUGIN_SHARED_REFERENCES):
        if not (repo_root / relative_path).exists():
            errors.append(f"Plugin shared reference is missing: {relative_path}")

    if not (repo_root / GITHUB_PREFLIGHT_SCRIPT).exists():
        errors.append(f"GitHub preflight script is missing: {GITHUB_PREFLIGHT_SCRIPT}")
    for relative_path in (HOME_LOCAL_INSTALL_HELPER, HOME_LOCAL_SMOKE_SCRIPT):
        if not (repo_root / relative_path).exists():
            errors.append(f"Plugin runtime script is missing: {relative_path}")

    for relative_path in sorted(REQUIRED_RUNTIME_DOCS):
        if not (repo_root / relative_path).exists():
            errors.append(f"Required runtime documentation is missing: {relative_path}")

    actual_skill_names = set()
    if not (repo_root / PLUGIN_SKILLS_ROOT).exists():
        errors.append(f"Plugin skills root is missing: {PLUGIN_SKILLS_ROOT}")
    else:
        actual_skill_names = {
            path.name
            for path in (repo_root / PLUGIN_SKILLS_ROOT).iterdir()
            if path.is_dir() and path.name != "_shared"
        }
        if actual_skill_names != PLUGIN_SKILL_CATALOG:
            errors.append(
                "Plugin skill catalog must match the expected skill surface: "
                f"{sorted(PLUGIN_SKILL_CATALOG)}"
            )

    runtime_skill_names = set(SKILL_SPECS)
    if not runtime_skill_names.issubset(PLUGIN_SKILL_CATALOG):
        errors.append(
            "Repo runtime skills must all resolve to plugin-owned skills: "
            f"{sorted(runtime_skill_names - PLUGIN_SKILL_CATALOG)}"
        )
    if set(SKILL_RUNTIME_CLASSIFICATIONS) != PLUGIN_SKILL_CATALOG:
        errors.append(
            "Plugin skill runtime classifications must cover the entire plugin skill catalog."
        )

    for skill_name in sorted(PLUGIN_SKILL_CATALOG):
        skill_root = repo_root / PLUGIN_SKILLS_ROOT / skill_name
        skill_doc = skill_root / "SKILL.md"
        metadata_path = skill_root / "agents" / "openai.yaml"
        if not skill_doc.exists():
            errors.append(f"Plugin skill doc is missing: {skill_doc.relative_to(repo_root)}")
            continue
        if not metadata_path.exists():
            errors.append(
                "Plugin skill metadata is missing: "
                f"{metadata_path.relative_to(repo_root)}"
            )
            continue
        skill_text = skill_doc.read_text(encoding="utf-8")

        try:
            metadata = _load_yaml(metadata_path)
        except ValueError as exc:
            errors.append(str(exc))
            continue

        interface = metadata.get("interface")
        if not isinstance(interface, dict):
            errors.append(f"{metadata_path.relative_to(repo_root)} must define interface metadata.")
            continue
        for key in ("display_name", "short_description", "default_prompt"):
            value = interface.get(key)
            if not isinstance(value, str) or not value.strip():
                errors.append(
                    f"{metadata_path.relative_to(repo_root)} must define interface.{key}."
                )
        default_prompt = interface.get("default_prompt")
        if isinstance(default_prompt, str) and f"${skill_name}" not in default_prompt:
            errors.append(
                f"{metadata_path.relative_to(repo_root)} must mention ${skill_name} "
                "in interface.default_prompt."
            )

        policy = metadata.get("policy")
        if not isinstance(policy, dict):
            errors.append(f"{metadata_path.relative_to(repo_root)} must define policy metadata.")
        elif not isinstance(policy.get("allow_implicit_invocation"), bool):
            errors.append(
                f"{metadata_path.relative_to(repo_root)} must define "
                "policy.allow_implicit_invocation as a boolean."
            )

        dependencies = metadata.get("dependencies", {})
        tools = dependencies.get("tools") if isinstance(dependencies, dict) else None
        github_touching = isinstance(tools, list) and any(
            isinstance(tool, dict)
            and tool.get("type") == "connector"
            and tool.get("value") == "github"
            for tool in tools
        )
        classification = SKILL_RUNTIME_CLASSIFICATIONS.get(skill_name)
        if classification not in ALLOWED_SKILL_RUNTIME_CLASSIFICATIONS:
            errors.append(
                f"{skill_name} must use a valid runtime classification: "
                f"{sorted(ALLOWED_SKILL_RUNTIME_CLASSIFICATIONS)}"
            )
            continue
        if classification == "github_required":
            if not github_touching:
                errors.append(
                    f"{metadata_path.relative_to(repo_root)} must declare the GitHub connector "
                    "dependency."
                )
            if GITHUB_PREREQUISITE_REFERENCE_SKILL_PATH not in skill_text:
                errors.append(
                    f"{skill_doc.relative_to(repo_root)} must reference "
                    f"{GITHUB_PREREQUISITE_REFERENCE}."
                )
            if "preflight_github_surface.py" not in skill_text:
                errors.append(
                    f"{skill_doc.relative_to(repo_root)} must reference "
                    f"{GITHUB_PREFLIGHT_SCRIPT}."
                )
        else:
            if github_touching:
                errors.append(
                    f"{metadata_path.relative_to(repo_root)} must not declare the GitHub "
                    "connector dependency for a non-required skill."
                )
            if GITHUB_PREREQUISITE_REFERENCE_SKILL_PATH in skill_text:
                errors.append(
                    f"{skill_doc.relative_to(repo_root)} must not hard-require "
                    "GitHub runtime prerequisites."
                )
            if "preflight_github_surface.py" in skill_text:
                errors.append(
                    f"{skill_doc.relative_to(repo_root)} must not hard-require "
                    f"{GITHUB_PREFLIGHT_SCRIPT}."
                )
            if classification == "local_first" and "## Optional Publish" not in skill_text:
                errors.append(
                    f"{skill_doc.relative_to(repo_root)} must keep GitHub publication, if any, "
                    "as an optional appendix."
                )
        if classification != "local_first" and "local-only" in skill_text.lower():
            errors.append(
                f"{skill_doc.relative_to(repo_root)} must not claim local-only behavior "
                "while using a GitHub runtime classification."
            )

    for workflow_name, assets in sorted(WORKFLOW_ASSETS.items()):
        workflow_path = repo_root / ".github" / "workflows" / workflow_name
        if not workflow_path.exists():
            errors.append(f"Workflow is missing: .github/workflows/{workflow_name}")
            continue
        workflow_text = workflow_path.read_text(encoding="utf-8")
        for kind, relative_path in assets.items():
            asset_path = repo_root / relative_path
            if not asset_path.exists():
                errors.append(f"Workflow asset is missing: {relative_path}")
            elif relative_path not in workflow_text:
                errors.append(
                    f".github/workflows/{workflow_name} must reference {kind} {relative_path}."
                )

    live_docs_paths = [
        repo_root / "AGENTS.md",
        repo_root / ".agents" / "README.md",
        repo_root / "README.md",
        repo_root / "plugins" / "project-management-workflows" / "README.md",
        repo_root / "plugins" / "project-management-workflows" / "skills" / "README.md",
        repo_root / ".github" / "ISSUE_TEMPLATE" / "config.yml",
        *sorted((repo_root / ".github" / "codex" / "prompts").glob("*.md")),
    ]
    for path in live_docs_paths:
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8")
        if REMOVED_DOCS_PREFIX in text:
            errors.append(
                f"{path.relative_to(repo_root)} must not reference removed engineering-doc paths."
            )

    plugin_readme = repo_root / "plugins" / "project-management-workflows" / "README.md"
    if plugin_readme.exists():
        readme_text = plugin_readme.read_text(encoding="utf-8")
        if "a custom app mapping" in readme_text and "does **not** bundle" in readme_text:
            errors.append(
                "plugins/project-management-workflows/README.md must not claim the plugin "
                "lacks a custom app mapping."
            )
        if "GitHub connector mapping" not in readme_text:
            errors.append(
                "plugins/project-management-workflows/README.md must describe the bundled "
                "GitHub connector mapping."
            )
        if "proving ground" in readme_text:
            errors.append(
                "plugins/project-management-workflows/README.md must not frame the plugin "
                "as a proving ground."
            )
        if HOME_LOCAL_INSTALL_HELPER.split("/")[-1] not in readme_text:
            errors.append(
                "plugins/project-management-workflows/README.md must document the home-local "
                "install helper."
            )

    skills_readme = repo_root / "plugins" / "project-management-workflows" / "skills" / "README.md"
    if skills_readme.exists():
        skills_text = skills_readme.read_text(encoding="utf-8")
        if "Runtime Classifications" not in skills_text:
            errors.append(
                "plugins/project-management-workflows/skills/README.md must document the "
                "skill runtime classifications."
            )

    plugin_manifest_text = (
        plugin_manifest.read_text(encoding="utf-8") if plugin_manifest.exists() else ""
    )
    if "proving ground" in plugin_manifest_text:
        errors.append("Repo-local plugin manifest must not frame the plugin as a proving ground.")
    if (repo_root / "plugins" / "project-management-workflows" / ".mcp.json").exists():
        errors.append("project-management-workflows must not bundle .mcp.json in this wave.")

    return errors


__all__ = [
    "AGENT_SKILL_BINDINGS",
    "ALLOWED_SKILL_RUNTIME_CLASSIFICATIONS",
    "EXPECTED_GITHUB_CONNECTOR_ID",
    "GITHUB_PREFLIGHT_SCRIPT",
    "GITHUB_PREREQUISITE_REFERENCE",
    "GITHUB_PREREQUISITE_REFERENCE_SKILL_PATH",
    "HOME_LOCAL_INSTALL_HELPER",
    "HOME_LOCAL_SMOKE_SCRIPT",
    "LOCAL_FIRST_SKILLS",
    "PLUGIN_SKILL_CATALOG",
    "PLUGIN_SKILLS_ROOT",
    "PLUGIN_SHARED_REFERENCES",
    "ROOT_AGENTS",
    "SKILL_RUNTIME_CLASSIFICATIONS",
    "WORKFLOW_ASSETS",
    "validate_codex_surface",
]
