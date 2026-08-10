use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_yaml::Value as YamlValue;

const RELEASE_WORKFLOW_FRAGMENTS: [&str; 10] = [
    "00-header.yml",
    "10-candidate.yml",
    "20-candidate-targets.yml",
    "30-candidate-distribution.yml",
    "40-candidate-homebrew-audit.yml",
    "50-publish-crate.yml",
    "60-build.yml",
    "70-draft-release.yml",
    "80-draft-action-smoke.yml",
    "90-marketplace-ready.yml",
];

const CODEX_WORKFLOWS: [&str; 4] = [
    "dependency-remediation.yml",
    "docs-taxonomy.yml",
    "governance-reconcile.yml",
    "merge-on-green.yml",
];

const AGENT_PLUGIN_WORKFLOWS: [&str; 5] = [
    "dependency-remediation.yml",
    "docs-taxonomy.yml",
    "governance-reconcile.yml",
    "merge-on-green.yml",
    "execution_state_sync.yml",
];

const AGENT_PLUGIN_WRAPPER: &str = "scripts/with-agent-plugins.sh";
const PREPARE_COMMAND: &str = "scripts/with-agent-plugins.sh --prepare";
const VERIFY_COMMAND: &str = "scripts/with-agent-plugins.sh --verify";
const MARKETPLACE_COMMAND: &str = "scripts/with-agent-plugins.sh marketplace install";
const RELEASE_TARGETS: [&str; 7] = [
    "aarch64-apple-darwin",
    "aarch64-pc-windows-msvc",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
];
const RELEASE_TARGET_RUNNERS: [(&str, &str); 7] = [
    ("macos-15", "aarch64-apple-darwin"),
    ("windows-11-arm", "aarch64-pc-windows-msvc"),
    ("ubuntu-22.04-arm", "aarch64-unknown-linux-gnu"),
    ("macos-15-intel", "x86_64-apple-darwin"),
    ("windows-2025", "x86_64-pc-windows-msvc"),
    ("ubuntu-22.04", "x86_64-unknown-linux-gnu"),
    ("ubuntu-22.04", "x86_64-unknown-linux-musl"),
];
const RELEASE_CHECKOUT_ACTION: &str = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
const RELEASE_VALIDATION_EMAIL: &str = "git-slop-release-validation@users.noreply.github.com";
const PUBLIC_RELEASE_WORKFLOWS: [&str; 3] = [
    "release-publish.yml",
    "release-published.yml",
    "homebrew-handoff.yml",
];
const PRIVATE_RUNTIME_SURFACES: [&str; 8] = [
    "AGENT_PLUGINS_READ_TOKEN",
    "AGENT_PLUGINS_GIT_TOKEN",
    AGENT_PLUGIN_WRAPPER,
    ".agents/plugins/marketplace-source.json",
    "coreycoto/agent-plugins",
    "agent-plugins-marketplace",
    "agent-plugins-runtime",
    "marketplace install",
];
const CRATES_IO_VERSION_ENDPOINT: &str = "https://crates.io/api/v1/crates/git-slop/${VERSION}";
const CRATES_IO_RELEASE_USER_AGENT: &str =
    r#"--user-agent "git-slop-release-workflow/1 (https://github.com/coreycoto/git-slop)""#;
const CRATES_IO_VERSION_REQUESTS: usize = 4;
const CRATES_IO_AUTH_ACTION: &str =
    "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18";
const CRATES_IO_AUTH_STEP: &str = "Exchange GitHub OIDC identity for crates.io token";
const CRATES_IO_PUBLISH_STEP: &str = "Publish exact crates.io package";
const CRATES_IO_PUBLISH_CONDITION: &str =
    "needs.candidate.outputs.mode == 'publish' && steps.state.outputs.crate-exists != 'true'";
const CRATES_IO_TEMP_TOKEN: &str = "${{ steps.crates-io-auth.outputs.token }}";
const RELEASE_DISPATCH_AUTHORIZATION: &str = "Explicitly authorize publishing exact current main, or recover an already-published immutable crate.";

pub fn validate(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let workflows = repo_root.join(".github/workflows");

    for name in CODEX_WORKFLOWS {
        let Some(text) = read(&workflows.join(name), &mut errors) else {
            continue;
        };
        if name == "dependency-remediation.yml" {
            for trusted_snapshot in [
                r#"codex_home="$RUNNER_TEMP/codex-runtime/.codex""#,
                r#"cp .codex/config.toml "$codex_home/config.toml""#,
                r#"cp .codex/*.config.toml "$codex_home/""#,
                r#"cp -R .codex/agents/. "$codex_home/agents/""#,
                "cp .github/codex/prompts/dependency-remediation.md",
                "cp .github/codex/schemas/dependency-remediation.json",
                "prompt-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.md",
                "output-schema-file: ${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.json",
            ] {
                require(&text, trusted_snapshot, name, &mut errors);
            }
        } else {
            require(
                &text,
                r#"cp .codex/config.toml "$RUNNER_TEMP/codex-runtime/.codex/config.toml""#,
                name,
                &mut errors,
            );
            require(
                &text,
                r#"cp .codex/*.config.toml "$RUNNER_TEMP/codex-runtime/.codex/""#,
                name,
                &mut errors,
            );
        }
        require(&text, MARKETPLACE_COMMAND, name, &mut errors);
        require(
            &text,
            "codex-home: ${{ runner.temp }}/codex-runtime/.codex",
            name,
            &mut errors,
        );
        require(&text, r#""--profile","ci_mutation""#, name, &mut errors);
        require(&text, "cargo xtask validate-codex", name, &mut errors);
        forbid(&text, "uv sync", name, &mut errors);
        forbid(
            &text,
            "scripts/validate_codex_surface.py",
            name,
            &mut errors,
        );
    }

    validate_agent_plugin_runtime(&workflows, &mut errors);
    validate_public_release_workflows(repo_root, &mut errors);

    for name in ["docs-taxonomy.yml", "merge-on-green.yml"] {
        if let Some(text) = read(&workflows.join(name), &mut errors) {
            forbid(&text, "gpt-5.4-nano", name, &mut errors);
            require(&text, r#""--model","gpt-5.6-luna""#, name, &mut errors);
        }
    }

    validate_action_versions(repo_root, &workflows, &mut errors);
    validate_artifacts(&workflows, &mut errors);
    validate_dogfood(&workflows, &mut errors);
    validate_ci(repo_root, &workflows, &mut errors);

    errors
}

fn render_release_workflow(repo_root: &Path) -> Result<String> {
    let source_root = repo_root.join(".github/workflow-sources/release-publish");
    let mut rendered = String::new();
    for name in RELEASE_WORKFLOW_FRAGMENTS {
        let path = source_root.join(name);
        let fragment = fs::read_to_string(&path).with_context(|| {
            format!(
                "unable to read release workflow fragment {}",
                path.display()
            )
        })?;
        if !fragment.ends_with('\n') {
            bail!(
                "release workflow fragment {} must end with a newline",
                path.display()
            );
        }
        rendered.push_str(&fragment);
    }
    Ok(rendered)
}

pub fn generate_release_workflow(repo_root: &Path, check: bool) -> Result<()> {
    let output = repo_root.join(".github/workflows/release-publish.yml");
    let rendered = render_release_workflow(repo_root)?;
    if check {
        let current = fs::read_to_string(&output)
            .with_context(|| format!("unable to read generated workflow {}", output.display()))?;
        if current != rendered {
            bail!(
                "{} is stale; run cargo xtask generate-release-workflow",
                output.display()
            );
        }
        return Ok(());
    }
    fs::write(&output, rendered)
        .with_context(|| format!("unable to write generated workflow {}", output.display()))
}

fn validate_generated_release_workflow(repo_root: &Path, errors: &mut Vec<String>) {
    if let Err(error) = generate_release_workflow(repo_root, true) {
        errors.push(format!(
            "release-publish.yml generation contract failed: {error:#}"
        ));
    }
}

pub(crate) fn validate_public_release_workflows(repo_root: &Path, errors: &mut Vec<String>) {
    validate_generated_release_workflow(repo_root, errors);
    let workflows = repo_root.join(".github/workflows");
    let Some((publish_text, publish)) = load_workflow(&workflows, "release-publish.yml", errors)
    else {
        return;
    };
    let Some((relay_text, relay)) = load_workflow(&workflows, "release-published.yml", errors)
    else {
        return;
    };
    let Some((homebrew_text, homebrew)) = load_workflow(&workflows, "homebrew-handoff.yml", errors)
    else {
        return;
    };

    for (name, text) in [
        ("release-publish.yml", publish_text.as_str()),
        ("release-published.yml", relay_text.as_str()),
        ("homebrew-handoff.yml", homebrew_text.as_str()),
    ] {
        validate_no_private_runtime(name, text, errors);
    }
    validate_release_publish(&publish_text, &publish, errors);
    validate_release_relay(&relay_text, &relay, errors);
    validate_homebrew_handoff(&homebrew, errors);
}

fn load_workflow(
    workflows: &Path,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<(String, YamlValue)> {
    let text = read(&workflows.join(name), errors)?;
    match serde_yaml::from_str::<YamlValue>(&text) {
        Ok(payload) => Some((text, payload)),
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            None
        }
    }
}

fn validate_no_private_runtime(name: &str, text: &str, errors: &mut Vec<String>) {
    for forbidden in PRIVATE_RUNTIME_SURFACES {
        forbid(text, forbidden, name, errors);
    }
}

include!("workflows/publication.rs");
include!("workflows/receivers.rs");
include!("workflows/yaml.rs");
include!("workflows/ci.rs");
include!("workflows/tests.rs");
