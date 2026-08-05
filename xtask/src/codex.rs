mod runtime_manifest;
mod runtime_workflows;

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value as JsonValue, json};
use toml::Value as TomlValue;

const EXPECTED_PLUGIN_URL: &str = "https://github.com/coreycoto/agent-plugins.git";
const GIT_SLOP_MARKETPLACE: &str = ".agents/plugins/marketplace.json";
const GIT_SLOP_MARKETPLACE_NAME: &str = "git-slop-marketplace";
const GIT_SLOP_PLUGIN_ROOT: &str = "plugins/git-slop";
const GIT_SLOP_PLUGIN_DOC_NAME: &str = "`git-slop` Codex plugin";
const GIT_SLOP_PLUGIN_NAME: &str = "git-slop";
const GIT_SLOP_PLUGIN_VERSION: &str = "0.2.3";
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

const GIT_SLOP_PLUGIN_SKILLS: [&str; 5] = [
    "adopt-repo",
    "install-update",
    "interpret-results",
    "plan-maintenance",
    "run-report",
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

pub fn validate(repo_root: &Path, require_codex_cli: bool) -> Vec<String> {
    let mut errors = Vec::new();

    validate_codex_config(repo_root, &mut errors);
    validate_execpolicy(repo_root, require_codex_cli, &mut errors);
    validate_marketplaces(repo_root, &mut errors);
    validate_product_plugin(repo_root, &mut errors);
    validate_removed_surfaces(repo_root, &mut errors);
    validate_agents(repo_root, &mut errors);
    validate_workflow_assets(repo_root, &mut errors);
    validate_guidance(repo_root, &mut errors);
    runtime_manifest::validate_agent_plugin_wrapper(repo_root, &mut errors);
    runtime_workflows::validate_agent_plugin_workflows(repo_root, &mut errors);
    validate_release_workflow(repo_root, &mut errors);
    validate_product_documentation(repo_root, &mut errors);

    errors
}

fn validate_codex_config(repo_root: &Path, errors: &mut Vec<String>) {
    let config_path = ".codex/config.toml";
    if let Some(payload) = load_toml(repo_root, config_path, errors) {
        if toml_string(&payload, "approval_policy") != Some("on-request") {
            errors.push(".codex/config.toml must default approval_policy to on-request.".into());
        }
        if toml_string(&payload, "sandbox_mode") != Some("workspace-write") {
            errors.push(".codex/config.toml must default sandbox_mode to workspace-write.".into());
        }
        if payload.get("profile").is_some() {
            errors.push(
                ".codex/config.toml must not define the legacy profile selector; select a \
                 standalone profile with --profile."
                    .into(),
            );
        }
        if payload.get("profiles").is_some() {
            errors.push(
                ".codex/config.toml must not define legacy [profiles.*] tables; use standalone \
                 .codex/<profile>.config.toml files."
                    .into(),
            );
        }
    }

    for (profile_name, expected_sandbox_mode) in CI_PROFILES {
        let relative = format!(".codex/{profile_name}.config.toml");
        let Some(payload) = load_toml(repo_root, &relative, errors) else {
            continue;
        };
        if toml_string(&payload, "approval_policy") != Some("never") {
            errors.push(format!(
                "{relative} must set top-level approval_policy to never."
            ));
        }
        if toml_string(&payload, "sandbox_mode") != Some(expected_sandbox_mode) {
            errors.push(format!(
                "{relative} must set top-level sandbox_mode to {expected_sandbox_mode}."
            ));
        }
    }
}

fn validate_execpolicy(repo_root: &Path, require_codex_cli: bool, errors: &mut Vec<String>) {
    let relative_rule_path = ".codex/rules/git.rules";
    let rule_path = repo_root.join(relative_rule_path);
    if !rule_path.is_file() {
        errors.push(".codex/rules/git.rules is missing.".into());
        return;
    }

    if !command_on_path("codex") {
        if require_codex_cli {
            errors.push("codex CLI is required but not installed.".into());
        }
        return;
    }

    const COMMANDS: [&[&str]; 3] = [
        &["git", "push", "origin", "main"],
        &["gh", "release", "create", "v1.2.3"],
        &["gh", "pr", "merge", "123", "--squash"],
    ];

    for checked_command in COMMANDS {
        let output = Command::new("codex")
            .args(["execpolicy", "check", "--rules"])
            .arg(&rule_path)
            .arg("--")
            .args(checked_command)
            .current_dir(repo_root)
            .output();

        let rendered_command = checked_command.join(" ");
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                errors.push(format!(
                    "execpolicy check failed to start for {rendered_command}: {error}"
                ));
                continue;
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            errors.push(format!(
                "execpolicy check failed for {rendered_command}: {detail}"
            ));
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        match require_prompt_decision(&stdout) {
            Ok(()) => {}
            Err(error) => errors.push(format!(
                "execpolicy check returned an unsafe result for {rendered_command}: {error}"
            )),
        }
    }
}

fn parse_execpolicy_decision(stdout: &str) -> Result<String, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("stdout was empty".into());
    }
    let payload: JsonValue = serde_json::from_str(trimmed)
        .map_err(|error| format!("stdout was not valid JSON: {error}"))?;
    payload
        .get("decision")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "JSON output did not contain a string decision".into())
}

fn require_prompt_decision(stdout: &str) -> Result<(), String> {
    let decision = parse_execpolicy_decision(stdout)?;
    if decision == EXPECTED_EXEC_POLICY_DECISION {
        Ok(())
    } else {
        Err(format!(
            "decision was {decision:?}; expected {EXPECTED_EXEC_POLICY_DECISION:?}"
        ))
    }
}

fn validate_marketplaces(repo_root: &Path, errors: &mut Vec<String>) {
    runtime_manifest::validate_marketplace_source(repo_root, errors);

    if let Some(marketplace) = load_json(repo_root, GIT_SLOP_MARKETPLACE, errors) {
        if json_string(&marketplace, "name") != Some(GIT_SLOP_MARKETPLACE_NAME) {
            errors
                .push(".agents/plugins/marketplace.json must define git-slop-marketplace.".into());
        }
        let expected_plugins = json!([
            {
                "name": "git-slop",
                "source": {"source": "local", "path": "./plugins/git-slop"},
                "policy": {
                    "installation": "AVAILABLE",
                    "authentication": "ON_INSTALL"
                },
                "category": "Developer Tools"
            }
        ]);
        if marketplace.get("plugins") != Some(&expected_plugins) {
            errors.push(
                ".agents/plugins/marketplace.json must publish exactly the expected git-slop \
                 product plugin entry."
                    .into(),
            );
        }
    }
}

fn validate_product_plugin(repo_root: &Path, errors: &mut Vec<String>) {
    let manifest_path = "plugins/git-slop/.codex-plugin/plugin.json";
    if let Some(manifest) = load_json(repo_root, manifest_path, errors) {
        if json_string(&manifest, "name") != Some(GIT_SLOP_PLUGIN_NAME) {
            errors.push("git-slop plugin manifest must use name git-slop.".into());
        }
        if json_string(&manifest, "version") != Some(GIT_SLOP_PLUGIN_VERSION) {
            errors.push(format!(
                "git-slop plugin manifest must use version {GIT_SLOP_PLUGIN_VERSION}."
            ));
        }
        if json_string(&manifest, "skills") != Some("./skills/") {
            errors.push("git-slop plugin manifest must expose ./skills/.".into());
        }
    }

    let skills_root = repo_root.join(GIT_SLOP_PLUGIN_ROOT).join("skills");
    let expected = GIT_SLOP_PLUGIN_SKILLS.into_iter().collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    match fs::read_dir(&skills_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        errors.push(format!(
                            "Unable to inspect plugins/git-slop/skills: {error}"
                        ));
                        continue;
                    }
                };
                let path = entry.path();
                if path.join("SKILL.md").is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        actual.insert(name.to_owned());
                    }
                }
            }
        }
        Err(error) => errors.push(format!(
            "Unable to inspect plugins/git-slop/skills: {error}"
        )),
    }

    for skill_name in expected.difference(&actual.iter().map(String::as_str).collect()) {
        errors.push(format!("plugins/git-slop skill is missing: {skill_name}."));
    }
    let actual_refs = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if actual_refs != expected {
        errors.push(format!(
            "plugins/git-slop must expose exactly the expected skill directories; found {actual:?}."
        ));
    }
}

fn validate_removed_surfaces(repo_root: &Path, errors: &mut Vec<String>) {
    for relative in REMOVED_CONSUMER_PATHS {
        if repo_root.join(relative).exists() {
            errors.push(format!(
                "{relative} should have been removed from the Rust-only maintainer surface."
            ));
        }
    }

    match crate::distribution::repository_owned_py_files(repo_root) {
        Ok(paths) => errors.extend(
            paths
                .into_iter()
                .map(|path| format!("Repository-owned .py file must be removed: {path}.")),
        ),
        Err(error) => errors.push(error),
    }
}

fn validate_agents(repo_root: &Path, errors: &mut Vec<String>) {
    for agent in AGENTS {
        let Some(payload) = read_text(repo_root, agent.path, errors) else {
            errors.push(format!(
                "Missing custom agent file for {}: {}.",
                agent.name, agent.path
            ));
            continue;
        };
        if payload.contains("[[skills.config]]") {
            errors.push(format!("{} must not bind local plugin paths.", agent.path));
        }
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES {
            if payload.contains(forbidden) {
                errors.push(format!("{} must not reference {forbidden}.", agent.path));
            }
        }
        for skill in agent.skills {
            if !payload.contains(skill) {
                errors.push(format!("{} must mention {skill}.", agent.path));
            }
        }
    }
}

fn validate_workflow_assets(repo_root: &Path, errors: &mut Vec<String>) {
    for workflow in WORKFLOWS {
        let workflow_path = format!(".github/workflows/{}", workflow.name);
        let workflow_text = read_text(repo_root, &workflow_path, errors);
        let prompt_text = read_text(repo_root, workflow.prompt, errors);
        let schema_text = read_text(repo_root, workflow.schema, errors);

        if let Some(prompt_text) = prompt_text {
            if !prompt_text.contains(workflow.skill) {
                errors.push(format!(
                    "{} must mention {}.",
                    workflow.prompt, workflow.skill
                ));
            }
            if !prompt_text.contains(workflow.agent_file) {
                errors.push(format!(
                    "{} must mention {}.",
                    workflow.prompt, workflow.agent_file
                ));
            }
            for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES {
                if prompt_text.contains(forbidden) {
                    errors.push(format!(
                        "{} must not reference {forbidden}.",
                        workflow.prompt
                    ));
                }
            }
        }

        if let Some(schema_text) = schema_text {
            if let Err(error) = serde_json::from_str::<JsonValue>(&schema_text) {
                errors.push(format!("Unable to parse {}: {error}", workflow.schema));
            }
        }

        if workflow.uses_agent_plugins {
            if let Some(workflow_text) = workflow_text {
                if !workflow_text.contains(workflow.prompt) {
                    errors.push(format!(
                        "{} must invoke prompt file {}.",
                        workflow.name, workflow.prompt
                    ));
                }
                if !workflow_text.contains(workflow.schema) {
                    errors.push(format!(
                        "{} must invoke output schema {}.",
                        workflow.name, workflow.schema
                    ));
                }
            }
        }
    }
}

fn validate_guidance(repo_root: &Path, errors: &mut Vec<String>) {
    for relative in REQUIRED_GUIDANCE {
        let Some(text) = read_text(repo_root, relative, errors) else {
            continue;
        };
        if !text.contains(EXPECTED_PLUGIN_URL) && !text.contains("agent-plugins") {
            errors.push(format!(
                "{relative} must point readers to the agent-plugins source of truth."
            ));
        }
        if matches!(
            relative,
            "AGENTS.md" | ".agents/README.md" | ".codex/README.md"
        ) {
            if !text.contains("marketplace-source.json") {
                errors.push(format!(
                    "{relative} must mention .agents/plugins/marketplace-source.json."
                ));
            }
            if !text.contains(GIT_SLOP_PLUGIN_DOC_NAME) {
                errors.push(format!(
                    "{relative} must mention {GIT_SLOP_PLUGIN_DOC_NAME}."
                ));
            }
        }
        for forbidden in REMOVED_LOCAL_PLUGIN_REFERENCES {
            if text.contains(forbidden) {
                errors.push(format!("{relative} must not reference {forbidden}."));
            }
        }
    }
}

fn validate_release_workflow(repo_root: &Path, errors: &mut Vec<String>) {
    let contracts: [(&str, &[&str]); 3] = [
        (
            ".github/workflows/release-publish.yml",
            &[
                "workflow_dispatch:",
                "cargo publish -p git-slop --locked --no-verify",
                "cargo xtask verify-crate",
                "verified-registry-crate",
                "gh release create \"$TAG\" --draft --generate-notes --title \"$TAG\" --verify-tag",
                "marketplace-ready:",
                "published-release relay",
            ],
        ),
        (
            ".github/workflows/release-published.yml",
            &[
                "types: [published]",
                "release-manifest.json",
                "gh workflow run homebrew-handoff.yml",
                "--ref main",
            ],
        ),
        (
            ".github/workflows/homebrew-handoff.yml",
            &[
                "workflow_dispatch:",
                "environment: release",
                "secrets.HOMEBREW_TAP_DISPATCH_TOKEN",
                "https://static.crates.io/crates/git-slop/",
                "--repo coreycoto/homebrew-tap",
                "--ref main",
            ],
        ),
    ];
    for (relative, required) in contracts {
        let Some(text) = read_text(repo_root, relative, errors) else {
            continue;
        };
        let label = relative.trim_start_matches(".github/workflows/");
        for forbidden in [
            "AGENT_PLUGINS_READ_TOKEN",
            "AGENT_PLUGINS_GIT_TOKEN",
            runtime_manifest::AGENT_PLUGIN_WRAPPER,
            runtime_manifest::MARKETPLACE_SOURCE_MANIFEST,
            runtime_manifest::EXPECTED_RUNTIME_ARCHIVE,
            runtime_manifest::EXPECTED_RUNTIME_REPOSITORY,
            runtime_manifest::EXPECTED_MARKETPLACE_NAME,
            "agent-plugins-runtime",
            "coreycoto/agent-plugins",
        ] {
            if text.contains(forbidden) {
                errors.push(format!(
                    "{label} must keep public release publication decoupled from private \
                     agent-plugins runtime surface {forbidden}."
                ));
            }
        }
        for required in required {
            if !text.contains(required) {
                errors.push(format!("{label} must include {required}."));
            }
        }
        for removed in [
            "scripts/build_release_manifest.py",
            "scripts/release_prepare.py",
            "scripts/update_homebrew_formula.py",
            "scripts/validate_codex_surface.py",
        ] {
            if text.contains(removed) {
                errors.push(format!(
                    "{label} must not reference retired helper {removed}."
                ));
            }
        }
    }
}

fn validate_product_documentation(repo_root: &Path, errors: &mut Vec<String>) {
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/run-report/SKILL.md",
        &[
            "git-slop health --report <report.json>",
            "git-slop health --format json",
            "writes its selected rendering to stdout",
            "does not rewrite `health.md`",
            "Use `check`",
            "references/health.md",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/run-report/references/health.md",
        &[
            "Every format writes to stdout",
            "does not rewrite that file",
            "Exit `0` means the selected report rendered successfully",
            "run `git-slop find` exactly once",
            "git-slop health --report path/to/report.json --format json",
            "git-slop health --report",
            "git-slop check --report",
            "does not modify report artifacts",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/adopt-repo/SKILL.md",
        &["actions/checkout@v7", "run `find` once"],
        &["actions/checkout@v6"],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "plugins/git-slop/skills/interpret-results/SKILL.md",
        &["Treat health output as advisory"],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "docs/commands.md",
        &[
            "Every format writes to standard output",
            "never rewrites `.slop/latest/health.md`",
            "successful rendering exits 0",
            "Use `git-slop check`",
            "# Repository Health",
            "git-slop explain --path src/parser.rs",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "docs/report-contract.md",
        &[
            "All three `health` formats write to standard output",
            "do not rewrite `.slop/latest/health.md`",
            "health.data_context_min_bytes",
            "health.folder_bands.refactor_required_max_direct_files",
            "health.summary_top_folders",
        ],
        &[],
        errors,
    );
    validate_normalized_contract(
        repo_root,
        "docs/github-action.md",
        &[
            "Run `git-slop find` once",
            "git-slop health --report",
            "`health` render exits 0",
        ],
        &[],
        errors,
    );
}

fn validate_normalized_contract(
    repo_root: &Path,
    relative: &str,
    required: &[&str],
    forbidden: &[&str],
    errors: &mut Vec<String>,
) {
    let Some(text) = read_text(repo_root, relative, errors) else {
        return;
    };
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    for expected in required {
        if !normalized.contains(expected) {
            errors.push(format!("{relative} must include {expected}."));
        }
    }
    for unexpected in forbidden {
        if normalized.contains(unexpected) {
            errors.push(format!("{relative} must not include {unexpected}."));
        }
    }
}

fn load_toml(repo_root: &Path, relative: &str, errors: &mut Vec<String>) -> Option<TomlValue> {
    let text = read_text(repo_root, relative, errors)?;
    match toml::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("Unable to parse {relative}: {error}"));
            None
        }
    }
}

fn load_json(repo_root: &Path, relative: &str, errors: &mut Vec<String>) -> Option<JsonValue> {
    let text = read_text(repo_root, relative, errors)?;
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("Unable to parse {relative}: {error}"));
            None
        }
    }
}

fn read_text(repo_root: &Path, relative: &str, errors: &mut Vec<String>) -> Option<String> {
    match fs::read_to_string(repo_root.join(relative)) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            errors.push(format!("{relative} is missing."));
            None
        }
        Err(error) => {
            errors.push(format!("Unable to read {relative}: {error}"));
            None
        }
    }
}

fn toml_string<'a>(value: &'a TomlValue, key: &str) -> Option<&'a str> {
    value.get(key).and_then(TomlValue::as_str)
}

fn json_string<'a>(value: &'a JsonValue, key: &str) -> Option<&'a str> {
    value.get(key).and_then(JsonValue::as_str)
}

fn command_on_path(command: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path)
        .any(|directory| command_candidates(&directory, command).any(|path| path.is_file()))
}

#[cfg(not(windows))]
fn command_candidates(directory: &Path, command: &str) -> impl Iterator<Item = PathBuf> {
    [directory.join(command)].into_iter()
}

#[cfg(windows)]
fn command_candidates(directory: &Path, command: &str) -> impl Iterator<Item = PathBuf> {
    [
        directory.join(command),
        directory.join(format!("{command}.exe")),
        directory.join(format!("{command}.cmd")),
        directory.join(format!("{command}.bat")),
    ]
    .into_iter()
}
