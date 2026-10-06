use std::path::Path;

use serde_yaml::Value as YamlValue;

use super::{WORKFLOWS, read_text};

mod codex_action;
mod dependency_remediation;
mod execution_state;
pub(super) use dependency_remediation::validate_dependency_candidate_artifact_ids;
pub(super) use execution_state::validate_policy_text as validate_execution_policy_text;

pub(super) const GH_STEWARD_PREPARE: &str = "scripts/with-gh-steward.sh --prepare";
pub(super) const GH_STEWARD_VERIFY: &str = "scripts/with-gh-steward.sh --verify";
pub(super) const CODEX_PLUGIN_SETUP: &str = "scripts/prepare-codex-plugins.sh";
pub(super) const CODEX_ACTION: &str =
    "openai/codex-action@86365089eb2b84e0a8fb0717b304f8bdcb13b20e";
pub(super) const VALIDATE_COMMAND: &str = "cargo xtask validate-codex";
const CODEX_CONFIG_COPY_COMMAND: &str =
    "cp .codex/config.toml \"$RUNNER_TEMP/codex-runtime/.codex/config.toml\"";
const CODEX_PROFILE_COPY_COMMAND: &str =
    "cp .codex/*.config.toml \"$RUNNER_TEMP/codex-runtime/.codex/\"";
const CODEX_HOME_INPUT: &str = "codex-home: ${{ runner.temp }}/codex-runtime/.codex";
const CODEX_APPROVAL_OVERRIDE: &str =
    "sed -i 's/^approval_policy = \"on-request\"$/approval_policy = \"never\"/'";
const PROJECT_SNAPSHOT: &str = "\"$GH_STEWARD_BIN\" snapshot project";
const EXECUTION_PREPARE: &str = "\"$GH_STEWARD_BIN\" execution prepare";
const EXECUTION_APPLY: &str = "\"$GH_STEWARD_BIN\" execution apply";
const PROJECT_TOKEN: &str =
    "${{ secrets.GH_PROJECTS_TOKEN != '' && secrets.GH_PROJECTS_TOKEN || github.token }}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AgentPluginWorkflowKind {
    CodexPlugins,
    ExecutionState,
}

struct WorkflowStepView {
    job: String,
    ordinal: usize,
    uses: String,
    run: String,
    checkout_ref: String,
    persist_credentials: Option<bool>,
    raw: YamlValue,
}

pub(super) fn validate_agent_plugin_workflows(repo_root: &Path, errors: &mut Vec<String>) {
    for workflow in WORKFLOWS
        .iter()
        .filter(|workflow| workflow.uses_agent_plugins)
    {
        let relative = format!(".github/workflows/{}", workflow.name);
        let Some(text) = read_text(repo_root, &relative, errors) else {
            continue;
        };
        validate_agent_plugin_workflow_text(
            workflow.name,
            &text,
            AgentPluginWorkflowKind::CodexPlugins,
            errors,
        );
        for (required, description) in [
            (VALIDATE_COMMAND, "run the Rust Codex surface validator"),
            (CODEX_ACTION, "invoke the immutable Codex action"),
            (
                CODEX_APPROVAL_OVERRIDE,
                "set noninteractive approval in the trusted temporary Codex config",
            ),
            (
                CODEX_HOME_INPUT,
                "pass the isolated Codex home to codex-action",
            ),
        ] {
            let present =
                if workflow.name == "dependency-remediation.yml" && required == VALIDATE_COMMAND {
                    text.contains("\"$GIT_SLOP_XTASK_BIN\" validate-codex")
                } else {
                    text.contains(required)
                };
            if !present {
                errors.push(format!("{} must {description}.", workflow.name));
            }
        }
        if workflow.name != "dependency-remediation.yml" {
            for (required, description) in [
                (
                    "$RUNNER_TEMP/codex-runtime/.codex",
                    "prepare a temporary isolated Codex home",
                ),
                (
                    CODEX_CONFIG_COPY_COMMAND,
                    "copy repo Codex config into the isolated Codex home",
                ),
                (
                    CODEX_PROFILE_COPY_COMMAND,
                    "copy standalone Codex profiles into the isolated Codex home",
                ),
            ] {
                if !text.contains(required) {
                    errors.push(format!("{} must {description}.", workflow.name));
                }
            }
        }
        if workflow.name == "dependency-remediation.yml"
            && let Some(proposal) = read_text(
                repo_root,
                "xtask/src/dependency_remediation/proposal.rs",
                errors,
            )
        {
            for required in ["source.patch", "source-verification.json"] {
                if !proposal.contains(required) {
                    errors.push(format!(
                        "dependency-remediation trusted proposal adapter must preserve exact {required} handoff bytes."
                    ));
                }
            }
        }
    }

    if let Some(source) = read_text(
        repo_root,
        "xtask/src/dependency_remediation/candidate.rs",
        errors,
    ) {
        validate_dependency_candidate_artifact_ids(&source, errors);
    }
    if let Some(source) = read_text(
        repo_root,
        "xtask/src/dependency_remediation/source.rs",
        errors,
    ) {
        validate_dependency_source_credential_boundary(&source, errors);
    }
    if let Some(policy) = read_text(repo_root, ".agents/gh-steward-recovery-policy.json", errors) {
        if let Some(workflow) = read_text(
            repo_root,
            ".github/workflows/dependency-remediation.yml",
            errors,
        ) {
            validate_dependency_publication_policy_text(&policy, &workflow, errors);
        }
        if let Some(workflow) = read_text(
            repo_root,
            ".github/workflows/execution_state_sync.yml",
            errors,
        ) {
            validate_execution_policy_text(&policy, &workflow, errors);
        }
    }

    let relative = ".github/workflows/execution_state_sync.yml";
    if let Some(text) = read_text(repo_root, relative, errors) {
        validate_agent_plugin_workflow_text(
            "execution_state_sync.yml",
            &text,
            AgentPluginWorkflowKind::ExecutionState,
            errors,
        );
    }
}

fn validate_dependency_source_credential_boundary(source: &str, errors: &mut Vec<String>) {
    for credential in ["GH_TOKEN", "GITHUB_TOKEN", "OPENAI_API_KEY"] {
        if !source.contains(&format!(".env_remove(\"{credential}\")")) {
            errors.push(format!(
                "dependency-remediation source verification must remove {credential} before running source tests."
            ));
            return;
        }
    }
}

pub(super) fn validate_dependency_publication_policy_text(
    source: &str,
    workflow_source: &str,
    errors: &mut Vec<String>,
) {
    let policy: serde_json::Value = match serde_json::from_str(source) {
        Ok(policy) => policy,
        Err(error) => {
            errors.push(format!(
                "dependency-remediation gh-steward policy is invalid JSON: {error}"
            ));
            return;
        }
    };
    let workflow = &policy["workflows"]["dependency-remediation.yml"];
    if workflow["allow_publication"] != serde_json::Value::Bool(false) {
        errors.push(
            "dependency-remediation publication must remain disabled until gh-steward owns the GitHub publication plan, apply, and receipt."
                .to_owned(),
        );
    }
    let mut alternatives = workflow["mutator_step_alternatives"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|alternative| alternative.as_array().into_iter().flatten());
    if alternatives.any(|step| {
        step.as_str().is_some_and(|step| {
            step == "Continue only the exact positively recoverable publication stage"
        })
    }) {
        errors.push(
            "dependency-remediation recovery policy must not authorize a direct Rust publication continuation."
                .to_owned(),
        );
    }
    if workflow["plans"]["workflow-noop"]["approval"]["mutators"]
        != serde_json::json!([{
            "job": "Settle an exact no-publication decision",
            "steps": ["Settle terminal decision through gh-steward"]
        }])
    {
        errors.push(
            "dependency-remediation no-op approval must bind the native terminal no-op step."
                .to_owned(),
        );
    }
    use sha2::{Digest, Sha256};
    let expected_digest = hex::encode(Sha256::digest(workflow_source.as_bytes()));
    if workflow["plans"]["workflow-noop"]["approval"]["workflow_source_sha256"] != expected_digest {
        errors.push(
            "dependency-remediation no-op approval must bind the exact current workflow bytes."
                .to_owned(),
        );
    }
}

pub(super) fn validate_agent_plugin_workflow_text(
    name: &str,
    text: &str,
    kind: AgentPluginWorkflowKind,
    errors: &mut Vec<String>,
) {
    if kind == AgentPluginWorkflowKind::CodexPlugins && !text.contains(CODEX_PLUGIN_SETUP) {
        errors.push(format!(
            "{name} must install the selected public plugins into isolated Codex state."
        ));
    }
    let model = match name {
        "governance-reconcile.yml" | "merge-on-green.yml" => "gpt-6.1-sol",
        _ => "gpt-6-luna",
    };
    if kind == AgentPluginWorkflowKind::CodexPlugins && !text.contains(model) {
        errors.push(format!("{name} must use the pinned {model} model."));
    }
    if kind == AgentPluginWorkflowKind::ExecutionState
        && [PROJECT_SNAPSHOT, EXECUTION_PREPARE, EXECUTION_APPLY]
            .iter()
            .any(|command| !text.contains(command))
    {
        errors.push(format!(
            "{name} must use the verified native binary for Project snapshots and reviewed execution prepare/apply."
        ));
    }

    for forbidden in [
        "agent-plugins-private-history",
        "AGENT_PLUGINS_READ_TOKEN",
        "AGENT_PLUGINS_GIT_TOKEN",
        "PEX_INTERPRETER",
        "python -m agent_plugins",
        "python -c \"from agent_plugins",
        "actions/setup-python",
        "python-version:",
        "uv run",
        "uv sync",
        "actions/cache@",
        "RUNNER_TOOL_CACHE",
        "runner.tool_cache",
        "restore-keys:",
    ] {
        if text.contains(forbidden) {
            errors.push(format!("{name} must not include {forbidden}."));
        }
    }

    let payload = match serde_yaml::from_str::<YamlValue>(text) {
        Ok(payload) => payload,
        Err(error) => {
            errors.push(format!("Unable to parse {name}: {error}"));
            return;
        }
    };
    let Some(steps) = workflow_step_views(&payload, name, errors) else {
        return;
    };
    validate_acquisition_scope(&payload, &steps, name, kind, errors);
    validate_step_order(&steps, name, kind, errors);
    if steps.iter().any(|step| step.run.contains("gh steward ")) {
        errors.push(format!("{name} must invoke the verified GH_STEWARD_BIN directly; a PATH entry does not register a GitHub CLI extension."));
    }
    codex_action::validate_args(&steps, name, errors);

    match name {
        "dependency-remediation.yml" => {
            dependency_remediation::validate_dependency_remediation_trust(
                text, &payload, &steps, errors,
            )
        }
        "execution_state_sync.yml" => {
            execution_state::validate_trust(text, &payload, &steps, errors);
            execution_state::validate_artifacts(text, &steps, errors);
        }
        _ => {}
    }
}

fn workflow_step_views(
    payload: &YamlValue,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<Vec<WorkflowStepView>> {
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!("{name} must define a jobs mapping."));
        return None;
    };
    let mut views = Vec::new();
    let mut ordinal = 0;
    for (job_name, job) in jobs {
        let job_name = job_name.as_str().unwrap_or("<non-string-job>");
        let Some(steps) = job.get("steps").and_then(YamlValue::as_sequence) else {
            errors.push(format!("{name} job {job_name} must define steps."));
            continue;
        };
        for step in steps {
            views.push(WorkflowStepView {
                job: job_name.to_owned(),
                ordinal,
                uses: step
                    .get("uses")
                    .and_then(YamlValue::as_str)
                    .unwrap_or("")
                    .to_owned(),
                run: step
                    .get("run")
                    .and_then(YamlValue::as_str)
                    .unwrap_or("")
                    .to_owned(),
                checkout_ref: step
                    .get("with")
                    .and_then(|value| value.get("ref"))
                    .and_then(YamlValue::as_str)
                    .unwrap_or("")
                    .to_owned(),
                persist_credentials: step
                    .get("with")
                    .and_then(|value| value.get("persist-credentials"))
                    .and_then(YamlValue::as_bool),
                raw: step.clone(),
            });
            ordinal += 1;
        }
    }
    Some(views)
}

fn validate_acquisition_scope(
    payload: &YamlValue,
    steps: &[WorkflowStepView],
    name: &str,
    kind: AgentPluginWorkflowKind,
    errors: &mut Vec<String>,
) {
    let prepares = steps
        .iter()
        .filter(|step| step.run.trim() == GH_STEWARD_PREPARE)
        .collect::<Vec<_>>();
    let verifies = steps
        .iter()
        .filter(|step| step.run.trim() == GH_STEWARD_VERIFY)
        .collect::<Vec<_>>();
    let plugin_setup = steps
        .iter()
        .filter(|step| step.run.contains(CODEX_PLUGIN_SETUP))
        .collect::<Vec<_>>();
    let codex = steps
        .iter()
        .filter(|step| step.uses == CODEX_ACTION)
        .collect::<Vec<_>>();
    let wants_plugins = kind == AgentPluginWorkflowKind::CodexPlugins;
    if (wants_plugins && plugin_setup.len() != 1)
        || (!wants_plugins && !plugin_setup.is_empty())
        || (wants_plugins && codex.len() != 1)
    {
        errors.push(format!(
            "{name} must have one dedicated native acquisition and verification pair, with isolated plugin install and Codex action only when needed."
        ));
        return;
    }
    let mut native_jobs = std::collections::BTreeSet::new();
    native_jobs.extend(prepares.iter().map(|step| step.job.as_str()));
    native_jobs.extend(verifies.iter().map(|step| step.job.as_str()));
    for job in &native_jobs {
        let job_prepares = prepares
            .iter()
            .filter(|step| step.job.as_str() == *job)
            .collect::<Vec<_>>();
        let job_verifies = verifies
            .iter()
            .filter(|step| step.job.as_str() == *job)
            .collect::<Vec<_>>();
        if job_prepares.len() != 1 || job_verifies.len() != 1 {
            errors.push(format!(
                "{name} job {job} must contain exactly one native acquisition and verification pair."
            ));
            continue;
        }
        if job_prepares[0].ordinal >= job_verifies[0].ordinal {
            errors.push(format!(
                "{name} job {job} must verify its acquired binary before using it."
            ));
        }
    }
    for step in &prepares {
        let env = step.raw.get("env").and_then(YamlValue::as_mapping);
        if env.is_none_or(|env| {
            env.len() != 1
                || env
                    .get(YamlValue::String("GH_TOKEN".into()))
                    .and_then(YamlValue::as_str)
                    != Some("${{ github.token }}")
        }) {
            errors.push(format!(
                "{name} native acquisition must receive only the step-scoped GitHub job token for attestation reads."
            ));
        }
    }
    for step in verifies.iter().chain(plugin_setup.iter()) {
        if !step
            .raw
            .get("env")
            .is_none_or(|env| env.as_mapping().is_some_and(|env| env.is_empty()))
            && step.raw.get("env").is_some_and(|env| {
                yaml_contains(env, "GH_TOKEN")
                    || yaml_contains(env, "GITHUB_TOKEN")
                    || yaml_contains(env, "secrets.")
            })
        {
            errors.push(format!(
                "{name} offline verification and isolated plugin setup must not receive GitHub tokens or other workflow secrets."
            ));
        }
    }
    if text_has_legacy_credentials_or_runtime(payload) {
        errors.push(format!(
            "{name} must not define legacy publisher tokens or the Python runtime anywhere in workflow structure."
        ));
    }
}

fn text_has_legacy_credentials_or_runtime(payload: &YamlValue) -> bool {
    [
        "AGENT_PLUGINS_READ_TOKEN",
        "AGENT_PLUGINS_GIT_TOKEN",
        "PEX_INTERPRETER",
        "agent-plugins-private-history",
    ]
    .iter()
    .any(|needle| yaml_contains(payload, needle))
}

fn validate_step_order(
    steps: &[WorkflowStepView],
    name: &str,
    kind: AgentPluginWorkflowKind,
    errors: &mut Vec<String>,
) {
    let jobs = steps
        .iter()
        .map(|step| step.job.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for job in jobs {
        let job_steps = steps
            .iter()
            .filter(|step| step.job == job)
            .collect::<Vec<_>>();
        let prepare = job_steps
            .iter()
            .find(|step| step.run.trim() == GH_STEWARD_PREPARE)
            .copied();
        let verify = job_steps
            .iter()
            .find(|step| step.run.trim() == GH_STEWARD_VERIFY)
            .copied();
        let native_commands = job_steps
            .iter()
            .filter(|step| {
                step.run.contains("\"$GH_STEWARD_BIN\" ")
                    && step.run.trim() != GH_STEWARD_PREPARE
                    && step.run.trim() != GH_STEWARD_VERIFY
            })
            .copied()
            .collect::<Vec<_>>();
        if !native_commands.is_empty() && (prepare.is_none() || verify.is_none()) {
            errors.push(format!(
                "{name} job {job} must acquire and verify gh-steward before running native commands."
            ));
        }
        if let (Some(prepare), Some(verify)) = (prepare, verify) {
            let checkout_before = job_steps.iter().any(|step| {
                step.ordinal < prepare.ordinal
                    && step.uses.starts_with("actions/checkout@")
                    && step.persist_credentials == Some(false)
            });
            if !checkout_before {
                errors.push(format!(
                    "{name} job {job} must check out trusted source with persisted credentials disabled before native acquisition."
                ));
            }
            if prepare.ordinal >= verify.ordinal
                || native_commands
                    .iter()
                    .any(|step| step.ordinal <= verify.ordinal)
            {
                errors.push(format!(
                    "{name} job {job} must verify the acquired binary before running native commands."
                ));
            }
        }
    }

    if kind == AgentPluginWorkflowKind::CodexPlugins {
        let plugins = steps
            .iter()
            .find(|step| step.run.contains(CODEX_PLUGIN_SETUP));
        let codex = steps.iter().find(|step| step.uses == CODEX_ACTION);
        if let (Some(plugins), Some(codex)) = (plugins, codex)
            && (plugins.job != codex.job || plugins.ordinal >= codex.ordinal)
        {
            errors.push(format!(
                "{name} must install pinned plugins before Codex executes in the same read-only review job."
            ));
        }
    }

    if kind == AgentPluginWorkflowKind::ExecutionState {
        for command in [PROJECT_SNAPSHOT, EXECUTION_PREPARE, EXECUTION_APPLY] {
            for step in steps.iter().filter(|step| step.run.contains(command)) {
                let verify = steps.iter().find(|candidate| {
                    candidate.job == step.job && candidate.run.trim() == GH_STEWARD_VERIFY
                });
                if verify.is_none_or(|verify| verify.ordinal >= step.ordinal) {
                    errors.push(format!(
                        "{name} must run {command} only after verifying the exact tool acquisition in its job."
                    ));
                }
            }
        }
    }
}

fn validate_step_token(
    authorized: &WorkflowStepView,
    expected_token: &str,
    name: &str,
    errors: &mut Vec<String>,
) {
    let Some(env) = authorized.raw.get("env").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} authorized operation must receive a step-scoped GH_TOKEN."
        ));
        return;
    };
    let key = YamlValue::String("GH_TOKEN".into());
    let unexpected_credential = env.iter().any(|(env_key, value)| {
        let name = env_key.as_str().unwrap_or_default();
        env_key != &key
            && (name.ends_with("_TOKEN")
                || name.contains("SECRET")
                || yaml_contains(value, "secrets."))
    });
    if env.get(&key).and_then(YamlValue::as_str) != Some(expected_token) || unexpected_credential {
        errors.push(format!(
            "{name} authorized operation must receive its expected step-scoped GH_TOKEN and no other credentials."
        ));
    }
}

fn yaml_mapping_has_key(value: &YamlValue, key: &str) -> bool {
    value
        .as_mapping()
        .is_some_and(|mapping| mapping.contains_key(YamlValue::String(key.into())))
}

fn yaml_contains(value: &YamlValue, needle: &str) -> bool {
    match value {
        YamlValue::String(value) => value.contains(needle),
        YamlValue::Sequence(values) => values.iter().any(|value| yaml_contains(value, needle)),
        YamlValue::Mapping(values) => values
            .iter()
            .any(|(key, value)| yaml_contains(key, needle) || yaml_contains(value, needle)),
        YamlValue::Tagged(value) => yaml_contains(&value.value, needle),
        _ => false,
    }
}
