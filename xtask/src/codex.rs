mod runtime_manifest;
mod runtime_workflows;

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;

include!("codex/product_documentation.rs");

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value as JsonValue, json};
use toml::Value as TomlValue;

use crate::manifest::project_version;

const EXPECTED_PLUGIN_URL: &str = "https://github.com/coreycoto/agent-plugins.git";
const AGENT_PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
const GIT_SLOP_MARKETPLACE: &str = ".agents/plugins/marketplace.json";
const GIT_SLOP_MARKETPLACE_NAME: &str = "git-slop-marketplace";
const GIT_SLOP_PLUGIN_MANIFEST: &str = "plugins/git-slop/plugin.json";
const GIT_SLOP_PLUGIN_ROOT: &str = "plugins/git-slop";
const GIT_SLOP_PLUGIN_DOC_NAME: &str = "`git-slop` Agent Plugin";
const GIT_SLOP_PLUGIN_NAME: &str = "git-slop";
const GIT_SLOP_CODEX_COMPAT_MANIFEST: &str = "plugins/git-slop/.codex-plugin/plugin.json";
const GIT_SLOP_BRAND_COLOR: &str = "#6f42c1";
const GIT_SLOP_ICON: &str = "plugins/git-slop/assets/git-slop.svg";
const EXPECTED_EXEC_POLICY_DECISION: &str = "prompt";

const REQUIRED_GUIDANCE: [&str; 5] = [
    "AGENTS.md",
    ".codex/README.md",
    "config/github/README.md",
    "config/labels/README.md",
    ".agents/README.md",
];

const CI_PROFILES: [(&str, &str); 3] = [
    ("ci_readonly", "read-only"),
    ("ci_mutation", "workspace-write"),
    ("ci_release", "workspace-write"),
];

const GIT_SLOP_PLUGIN_SKILLS: [&str; 4] = [
    "adopt-repo",
    "install-update",
    "review-results",
    "run-report",
];

const GIT_SLOP_PLUGIN_CLIENTS: [&str; 5] = [
    "ChatGPT & Codex",
    "VS Code",
    "Cursor",
    "GitHub Copilot",
    "Kiro",
];

struct SkillContract {
    skill_name: &'static str,
    trigger_description: &'static str,
    display_name: &'static str,
    short_description: &'static str,
    default_prompt: &'static str,
}

const GIT_SLOP_SKILL_CONTRACTS: [SkillContract; 4] = [
    SkillContract {
        skill_name: "adopt-repo",
        trigger_description: "Integrate git-slop into a consumer repository through durable plugin source metadata, local configuration and wrapper conventions, and the advisory GitHub Action contract. Use when a user wants repository or CI adoption; use install-update instead when the outcome is only a native CLI installation.",
        display_name: "Adopt Git Slop",
        short_description: "Integrate Git Slop into repositories and CI",
        default_prompt: "Use $git-slop:adopt-repo to integrate Git Slop into this repository and its CI workflow.",
    },
    SkillContract {
        skill_name: "install-update",
        trigger_description: "Install, update, and verify the native git-slop CLI through Cargo, Homebrew, Scoop, or a checksummed release archive. Use when a machine needs a usable binary on PATH or an installed binary's version and source revision must be proven; repository configuration and GitHub Action adoption belong to the adopt-repo skill.",
        display_name: "Install Git Slop",
        short_description: "Install and verify the native Git Slop CLI",
        default_prompt: "Use $git-slop:install-update to install or update the native Git Slop CLI and verify its provenance.",
    },
    SkillContract {
        skill_name: "review-results",
        trigger_description: "Review existing git-slop report and health artifacts, explain detector and overlay evidence, and optionally turn one explicitly selected finding into a bounded maintenance proposal. Use when a user asks what results mean, which findings are actionable, or wants a plan from a reviewed finding; do not use merely to generate a fresh report.",
        display_name: "Review Git Slop Results",
        short_description: "Interpret Git Slop evidence and plan bounded work",
        default_prompt: "Use $git-slop:review-results to explain these Git Slop findings and, if requested, plan one bounded maintenance slice.",
    },
    SkillContract {
        skill_name: "run-report",
        trigger_description: "Generate and render git-slop reports, health output, comparisons, SARIF, and explicit checks while preserving local artifact and advisory CI conventions. Use when a user wants fresh or re-rendered detector output; use review-results to interpret findings or plan maintenance.",
        display_name: "Run Git Slop Reports",
        short_description: "Generate and render Git Slop health reports",
        default_prompt: "Use $git-slop:run-report to generate a fresh Git Slop report with the repository's preferred wrapper or native CLI.",
    },
];

const REMOVED_LOCAL_PLUGIN_REFERENCES: [&str; 3] = [
    "plugins/project-management-workflows/",
    "manage_home_local_plugin.py",
    "smoke_home_install.py",
];

const REMOVED_CONSUMER_PATHS: [&str; 14] = [
    "plugins/project-management-workflows",
    "scripts/bootstrap_agent_plugins_marketplace.py",
    "scripts/smoke_plugin_consumer.py",
    "tests/test_github_surface_preflight.py",
    "tests/test_plugin_home_install.py",
    "tests/test_agent_tools_integration.py",
    "tests/test_plugin_consumer_smoke.py",
    "tests/unit/agent_tools/test_backlog_deltas.py",
    "tests/unit/agent_tools/test_governance_config.py",
    "tests/unit/agent_tools/test_issue_forms.py",
    "tests/unit/agent_tools/test_research_digest.py",
    "pyproject.toml",
    "uv.lock",
    "src/git_slop/integrations/agents/codex_surface.py",
];

struct AgentContract {
    name: &'static str,
    path: &'static str,
    skills: &'static [&'static str],
}

const AGENTS: [AgentContract; 5] = [
    AgentContract {
        name: "dependency_patcher",
        path: ".codex/agents/dependency-patcher.toml",
        skills: &["$project-management-workflows:dependency-remediation"],
    },
    AgentContract {
        name: "docs_taxonomist",
        path: ".codex/agents/docs-taxonomist.toml",
        skills: &["$project-management-workflows:docs-taxonomy"],
    },
    AgentContract {
        name: "governance_auditor",
        path: ".codex/agents/governance-auditor.toml",
        skills: &[
            "$project-management-workflows:ensure-quarter-milestones",
            "$project-management-workflows:github-backlog-mutate",
            "$project-management-workflows:label-palette-design",
        ],
    },
    AgentContract {
        name: "merge_gatekeeper",
        path: ".codex/agents/merge-gatekeeper.toml",
        skills: &["$project-management-workflows:merge-on-green"],
    },
    AgentContract {
        name: "release_publisher",
        path: ".codex/agents/release-publisher.toml",
        skills: &["$project-management-workflows:release-publish"],
    },
];

struct WorkflowContract {
    name: &'static str,
    prompt: &'static str,
    schema: &'static str,
    skill: &'static str,
    agent_file: &'static str,
    uses_agent_plugins: bool,
}

const WORKFLOWS: [WorkflowContract; 5] = [
    WorkflowContract {
        name: "dependency-remediation.yml",
        prompt: ".github/codex/prompts/dependency-remediation.md",
        schema: ".github/codex/schemas/dependency-remediation.json",
        skill: "$project-management-workflows:dependency-remediation",
        agent_file: ".codex/agents/dependency-patcher.toml",
        uses_agent_plugins: true,
    },
    WorkflowContract {
        name: "docs-taxonomy.yml",
        prompt: ".github/codex/prompts/docs-taxonomy.md",
        schema: ".github/codex/schemas/docs-taxonomy.json",
        skill: "$project-management-workflows:docs-taxonomy",
        agent_file: ".codex/agents/docs-taxonomist.toml",
        uses_agent_plugins: true,
    },
    WorkflowContract {
        name: "governance-reconcile.yml",
        prompt: ".github/codex/prompts/governance-reconcile.md",
        schema: ".github/codex/schemas/governance-reconcile.json",
        skill: "$project-management-workflows:github-backlog-mutate",
        agent_file: ".codex/agents/governance-auditor.toml",
        uses_agent_plugins: true,
    },
    WorkflowContract {
        name: "merge-on-green.yml",
        prompt: ".github/codex/prompts/merge-on-green.md",
        schema: ".github/codex/schemas/merge-on-green.json",
        skill: "$project-management-workflows:merge-on-green",
        agent_file: ".codex/agents/merge-gatekeeper.toml",
        uses_agent_plugins: true,
    },
    WorkflowContract {
        name: "release-publish.yml",
        prompt: ".github/codex/prompts/release-publish.md",
        schema: ".github/codex/schemas/release-publish.json",
        skill: "$project-management-workflows:release-publish",
        agent_file: ".codex/agents/release-publisher.toml",
        uses_agent_plugins: false,
    },
];

include!("codex/validation/config.rs");
include!("codex/validation/plugin.rs");
include!("codex/validation/repository.rs");
include!("codex/validation/workflows.rs");
include!("codex/validation/documentation.rs");
include!("codex/validation/support.rs");
