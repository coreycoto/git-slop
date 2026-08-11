use std::path::Path;

use serde_yaml::Value as YamlValue;

use super::{WORKFLOWS, read_text};

pub(super) const PREPARE_COMMAND: &str = "scripts/with-agent-plugins.sh --prepare";
pub(super) const VERIFY_COMMAND: &str = "scripts/with-agent-plugins.sh --verify";
pub(super) const MARKETPLACE_COMMAND: &str = "scripts/with-agent-plugins.sh marketplace install";
pub(super) const PROJECT_SNAPSHOT_COMMAND: &str =
    "scripts/with-agent-plugins.sh github project-snapshot";
pub(super) const EXECUTION_STATE_COMMAND: &str =
    "scripts/with-agent-plugins.sh github execution-state";

const VALIDATE_COMMAND: &str = "cargo xtask validate-codex";
const CODEX_CONFIG_COPY_COMMAND: &str =
    "cp .codex/config.toml \"$RUNNER_TEMP/codex-runtime/.codex/config.toml\"";
const CODEX_PROFILE_COPY_COMMAND: &str =
    "cp .codex/*.config.toml \"$RUNNER_TEMP/codex-runtime/.codex/\"";
const CODEX_HOME_INPUT: &str = "codex-home: ${{ runner.temp }}/codex-runtime/.codex";
const CODEX_ACTION: &str = "openai/codex-action@52fe01ec70a42f454c9d2ebd47598f9fd6893d56";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AgentPluginWorkflowKind {
    Marketplace,
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
            AgentPluginWorkflowKind::Marketplace,
            errors,
        );
        for (required, description) in [
            (VALIDATE_COMMAND, "run the Rust Codex surface validator"),
            (CODEX_ACTION, "invoke the immutable Codex action"),
            (
                CODEX_HOME_INPUT,
                "pass the isolated Codex home to codex-action",
            ),
        ] {
            if !text.contains(required) {
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

pub(super) fn validate_agent_plugin_workflow_text(
    name: &str,
    text: &str,
    kind: AgentPluginWorkflowKind,
    errors: &mut Vec<String>,
) {
    for (required, description) in [
        (PREPARE_COMMAND, "explicitly acquire the pinned runtime"),
        (VERIFY_COMMAND, "verify the acquired runtime offline"),
        (
            "AGENT_PLUGINS_READ_TOKEN: ${{ secrets.AGENT_PLUGINS_READ_TOKEN }}",
            "scope the read-only token to runtime acquisition",
        ),
    ] {
        if !text.contains(required) {
            errors.push(format!("{name} must {description}."));
        }
    }

    match kind {
        AgentPluginWorkflowKind::Marketplace => {
            if !text.contains(MARKETPLACE_COMMAND) {
                errors.push(format!(
                    "{name} must install the marketplace through the direct runtime CLI."
                ));
            }
        }
        AgentPluginWorkflowKind::ExecutionState => {
            for (required, description) in [
                (
                    PROJECT_SNAPSHOT_COMMAND,
                    "run project-snapshot through the direct runtime CLI",
                ),
                (
                    EXECUTION_STATE_COMMAND,
                    "run execution-state through the direct runtime CLI",
                ),
            ] {
                if !text.contains(required) {
                    errors.push(format!("{name} must {description}."));
                }
            }
        }
    }

    for forbidden in [
        "actions/setup-python",
        "python-version:",
        "python -m pip",
        "pip install",
        "Install uv",
        "uv run",
        "uv sync",
        "AGENT_PLUGINS_GIT_TOKEN",
        "git ls-remote",
        "insteadOf",
        "python -m agent_plugins",
        "python -c \"from agent_plugins",
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
    validate_acquisition_token_scope(&payload, &steps, name, errors);
    validate_runtime_step_order(&steps, name, kind, errors);

    match name {
        "dependency-remediation.yml" => {
            validate_dependency_remediation_trust(text, &payload, &steps, errors)
        }
        "docs-taxonomy.yml" => validate_docs_taxonomy_artifacts(&steps, errors),
        "execution_state_sync.yml" => {
            validate_execution_state_trust(text, &payload, &steps, errors);
            validate_execution_state_artifacts(&steps, errors);
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

fn validate_acquisition_token_scope(
    payload: &YamlValue,
    steps: &[WorkflowStepView],
    name: &str,
    errors: &mut Vec<String>,
) {
    let acquisitions = steps
        .iter()
        .filter(|step| step.run.contains(PREPARE_COMMAND))
        .collect::<Vec<_>>();
    if acquisitions.len() != 1 {
        errors.push(format!(
            "{name} must define exactly one dedicated agent-plugins acquisition step."
        ));
        return;
    }
    let acquisition = acquisitions[0];
    if acquisition.run.trim() != PREPARE_COMMAND {
        errors.push(format!(
            "{name} acquisition step must run only {PREPARE_COMMAND}."
        ));
    }

    let Some(env) = acquisition.raw.get("env").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} acquisition step must define a step-scoped token environment."
        ));
        return;
    };
    let token_key = YamlValue::String("AGENT_PLUGINS_READ_TOKEN".into());
    let expected_secret = "${{ secrets.AGENT_PLUGINS_READ_TOKEN }}";
    if env.get(&token_key).and_then(YamlValue::as_str) != Some(expected_secret) {
        errors.push(format!(
            "{name} acquisition step must map AGENT_PLUGINS_READ_TOKEN directly from its \
             namesake secret."
        ));
    }
    if env.len() != 1 {
        errors.push(format!(
            "{name} acquisition step must expose only AGENT_PLUGINS_READ_TOKEN."
        ));
    }

    if let Some(mapping) = acquisition.raw.as_mapping() {
        for (key, value) in mapping {
            if key.as_str() != Some("env")
                && (yaml_contains(key, "AGENT_PLUGINS_READ_TOKEN")
                    || yaml_contains(value, "AGENT_PLUGINS_READ_TOKEN"))
            {
                errors.push(format!(
                    "{name} acquisition token must not appear outside the step env mapping."
                ));
                break;
            }
        }
    }
    for (key, value) in env {
        if key != &token_key
            && (yaml_contains(key, "AGENT_PLUGINS_READ_TOKEN")
                || yaml_contains(value, "AGENT_PLUGINS_READ_TOKEN"))
        {
            errors.push(format!(
                "{name} acquisition token must not be aliased through another environment key."
            ));
            break;
        }
    }

    for step in steps {
        if step.ordinal != acquisition.ordinal
            && yaml_contains(&step.raw, "AGENT_PLUGINS_READ_TOKEN")
        {
            errors.push(format!(
                "{name} must keep AGENT_PLUGINS_READ_TOKEN only in the dedicated acquisition step."
            ));
        }
    }
    if let Some(root) = payload.as_mapping() {
        for (key, value) in root {
            if key.as_str() == Some("jobs") {
                continue;
            }
            if yaml_contains(key, "AGENT_PLUGINS_READ_TOKEN")
                || yaml_contains(value, "AGENT_PLUGINS_READ_TOKEN")
            {
                errors.push(format!(
                    "{name} must not define AGENT_PLUGINS_READ_TOKEN outside job steps."
                ));
            }
        }
    }
    if let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) {
        for (_, job) in jobs {
            if let Some(job) = job.as_mapping() {
                for (key, value) in job {
                    if key.as_str() == Some("steps") {
                        continue;
                    }
                    if yaml_contains(key, "AGENT_PLUGINS_READ_TOKEN")
                        || yaml_contains(value, "AGENT_PLUGINS_READ_TOKEN")
                    {
                        errors.push(format!(
                            "{name} must not define AGENT_PLUGINS_READ_TOKEN at job scope."
                        ));
                    }
                }
            }
        }
    }
}

fn validate_runtime_step_order(
    steps: &[WorkflowStepView],
    name: &str,
    kind: AgentPluginWorkflowKind,
    errors: &mut Vec<String>,
) {
    let acquisitions = steps
        .iter()
        .filter(|step| step.run.contains(PREPARE_COMMAND))
        .collect::<Vec<_>>();
    let verifications = steps
        .iter()
        .filter(|step| step.run.contains(VERIFY_COMMAND))
        .collect::<Vec<_>>();
    if acquisitions.len() != 1 || verifications.len() != 1 {
        if verifications.len() != 1 {
            errors.push(format!(
                "{name} must define exactly one separate offline runtime verification step."
            ));
        }
        return;
    }
    let acquisition = acquisitions[0];
    let verification = verifications[0];
    if verification.run.trim() != VERIFY_COMMAND {
        errors.push(format!(
            "{name} verification step must run only {VERIFY_COMMAND}."
        ));
    }
    if acquisition.job != verification.job || acquisition.ordinal >= verification.ordinal {
        errors.push(format!(
            "{name} must verify the runtime after acquisition in the same job."
        ));
    }
    if yaml_contains(&verification.raw, "AGENT_PLUGINS_READ_TOKEN") {
        errors.push(format!(
            "{name} offline verification step must not receive AGENT_PLUGINS_READ_TOKEN."
        ));
    }

    let direct_commands = match kind {
        AgentPluginWorkflowKind::Marketplace => [MARKETPLACE_COMMAND, ""].as_slice(),
        AgentPluginWorkflowKind::ExecutionState => {
            [PROJECT_SNAPSHOT_COMMAND, EXECUTION_STATE_COMMAND].as_slice()
        }
    };
    for command in direct_commands.iter().filter(|command| !command.is_empty()) {
        let matches = steps
            .iter()
            .filter(|step| step.run.contains(command))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            errors.push(format!(
                "{name} must invoke {command} exactly once through the direct CLI."
            ));
            continue;
        }
        let direct = matches[0];
        if direct.job != verification.job || direct.ordinal <= verification.ordinal {
            errors.push(format!(
                "{name} must invoke {command} only after offline runtime verification."
            ));
        }
        if yaml_contains(&direct.raw, "AGENT_PLUGINS_READ_TOKEN") {
            errors.push(format!(
                "{name} direct runtime commands must not receive AGENT_PLUGINS_READ_TOKEN."
            ));
        }
    }

    let checkout_before_acquisition = steps.iter().any(|step| {
        step.job == acquisition.job
            && step.ordinal < acquisition.ordinal
            && step.uses.starts_with("actions/checkout@")
    });
    if !checkout_before_acquisition {
        errors.push(format!(
            "{name} must check out trusted wrapper and manifest content before acquisition."
        ));
    }
}

fn validate_dependency_remediation_trust(
    text: &str,
    payload: &YamlValue,
    steps: &[WorkflowStepView],
    errors: &mut Vec<String>,
) {
    let name = "dependency-remediation.yml";
    if !text.contains("github.actor == 'dependabot[bot]'") {
        errors.push(format!(
            "{name} must restrict pull_request_target execution to Dependabot."
        ));
    }
    let checkouts = steps
        .iter()
        .filter(|step| step.uses.starts_with("actions/checkout@"))
        .collect::<Vec<_>>();
    let Some(first) = checkouts.first() else {
        errors.push(format!("{name} must check out the trusted base revision."));
        return;
    };
    if checkouts.len() != 2 {
        errors.push(format!(
            "{name} must define exactly the trusted-base and requested-head checkouts."
        ));
    }
    if !first.checkout_ref.contains("pull_request.base.sha") {
        errors.push(format!(
            "{name} first checkout must select the trusted pull-request base revision."
        ));
    }
    if first.persist_credentials != Some(false) {
        errors.push(format!(
            "{name} trusted-base checkout must disable persisted GitHub credentials."
        ));
    }
    let head_checkouts = checkouts
        .iter()
        .filter(|step| step.checkout_ref.contains("pull_request.head.sha"))
        .collect::<Vec<_>>();
    if head_checkouts.len() != 1 {
        errors.push(format!(
            "{name} must define exactly one explicit requested-head checkout."
        ));
        return;
    }
    let head = head_checkouts[0];
    if head.persist_credentials != Some(false) {
        errors.push(format!(
            "{name} requested-head checkout must disable persisted GitHub credentials."
        ));
    }

    let validations = steps
        .iter()
        .filter(|step| step.run.trim() == VALIDATE_COMMAND)
        .collect::<Vec<_>>();
    let rust_setups = steps
        .iter()
        .filter(|step| step.uses.starts_with("dtolnay/rust-toolchain@"))
        .collect::<Vec<_>>();
    let acquisitions = steps
        .iter()
        .filter(|step| step.run.trim() == PREPARE_COMMAND)
        .collect::<Vec<_>>();
    let verifications = steps
        .iter()
        .filter(|step| step.run.trim() == VERIFY_COMMAND)
        .collect::<Vec<_>>();
    let snapshots = steps
        .iter()
        .filter(|step| step.run.contains("cp -R .codex/agents/."))
        .collect::<Vec<_>>();
    let marketplaces = steps
        .iter()
        .filter(|step| step.run.contains(MARKETPLACE_COMMAND))
        .collect::<Vec<_>>();
    let codex_steps = steps
        .iter()
        .filter(|step| step.uses == CODEX_ACTION)
        .collect::<Vec<_>>();
    if rust_setups.len() != 1
        || validations.len() != 1
        || acquisitions.len() != 1
        || verifications.len() != 1
        || snapshots.len() != 1
        || marketplaces.len() != 1
        || codex_steps.len() != 1
    {
        errors.push(format!(
            "{name} must define one trusted Rust setup, validation, acquisition, verification, \
             runtime-input snapshot, marketplace install, and Codex mutation step."
        ));
        return;
    }
    let rust_setup = rust_setups[0];
    let validation = validations[0];
    let acquisition = acquisitions[0];
    let verification = verifications[0];
    let snapshot = snapshots[0];
    let marketplace = marketplaces[0];
    let codex = codex_steps[0];
    let all_in_trusted_job = [
        rust_setup,
        validation,
        acquisition,
        verification,
        snapshot,
        marketplace,
        head,
        codex,
    ]
    .iter()
    .all(|step| step.job == first.job);
    if !all_in_trusted_job
        || !(first.ordinal < rust_setup.ordinal
            && rust_setup.ordinal < validation.ordinal
            && validation.ordinal < acquisition.ordinal
            && acquisition.ordinal < verification.ordinal
            && verification.ordinal < snapshot.ordinal
            && snapshot.ordinal < marketplace.ordinal
            && marketplace.ordinal < head.ordinal
            && head.ordinal < codex.ordinal)
    {
        errors.push(format!(
            "{name} must set up Rust, validate, acquire, verify, snapshot, and install from \
             trusted base content in one job before the requested-head checkout and Codex mutation."
        ));
    }

    for required in [
        "trusted_root=\"$RUNNER_TEMP/dependency-remediation-trusted\"",
        "codex_home=\"$RUNNER_TEMP/codex-runtime/.codex\"",
        "cp .codex/config.toml \"$codex_home/config.toml\"",
        "cp .codex/*.config.toml \"$codex_home/\"",
        "cp -R .codex/agents/. \"$codex_home/agents/\"",
        "cp .github/codex/prompts/dependency-remediation.md",
        "\"$trusted_root/dependency-remediation.md\"",
        "cp .github/codex/schemas/dependency-remediation.json",
        "\"$trusted_root/dependency-remediation.json\"",
    ] {
        if !snapshot.run.contains(required) {
            errors.push(format!(
                "{name} trusted runtime-input snapshot must include {required}."
            ));
        }
    }

    for step in steps.iter().filter(|step| step.ordinal > head.ordinal) {
        for forbidden in [
            "cargo xtask",
            ".codex/config.toml",
            ".codex/agents/",
            ".config.toml",
            ".github/codex/prompts/",
            ".github/codex/schemas/",
        ] {
            if step.run.contains(forbidden) {
                errors.push(format!(
                    "{name} must not use requested-head Codex inputs after checkout: {forbidden}."
                ));
            }
        }
    }

    let Some(codex_env) = codex.raw.get("env").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} Codex mutation step must receive a step-scoped GH_TOKEN."
        ));
        return;
    };
    let gh_token_key = YamlValue::String("GH_TOKEN".into());
    if codex_env.get(&gh_token_key).and_then(YamlValue::as_str) != Some("${{ github.token }}")
        || codex_env.len() != 1
    {
        errors.push(format!(
            "{name} Codex mutation step must expose only GH_TOKEN from github.token."
        ));
    }
    let root_exposes_token = payload.get("env").is_some_and(|env| {
        yaml_mapping_has_key(env, "GH_TOKEN") || yaml_contains(env, "github.token")
    });
    let job_exposes_token = payload
        .get("jobs")
        .and_then(YamlValue::as_mapping)
        .is_some_and(|jobs| {
            jobs.values().any(|job| {
                job.get("env").is_some_and(|env| {
                    yaml_mapping_has_key(env, "GH_TOKEN") || yaml_contains(env, "github.token")
                })
            })
        });
    if root_exposes_token || job_exposes_token {
        errors.push(format!(
            "{name} must not expose github.token through workflow- or job-level environment."
        ));
    }
    for step in steps.iter().filter(|step| step.ordinal != codex.ordinal) {
        if step
            .raw
            .get("env")
            .is_some_and(|env| yaml_mapping_has_key(env, "GH_TOKEN"))
            || yaml_contains(&step.raw, "github.token")
        {
            errors.push(format!(
                "{name} must expose github.token only to the deliberate Codex mutation step."
            ));
        }
    }

    let Some(inputs) = codex.raw.get("with") else {
        errors.push(format!(
            "{name} Codex mutation step must define trusted inputs."
        ));
        return;
    };
    let allowed_inputs = [
        "allow-bot-users",
        "codex-args",
        "codex-home",
        "openai-api-key",
        "output-file",
        "output-schema-file",
        "prompt-file",
        "safety-strategy",
        "sandbox",
    ];
    if let Some(mapping) = inputs.as_mapping() {
        for key in mapping.keys().filter_map(YamlValue::as_str) {
            if !allowed_inputs.contains(&key) {
                errors.push(format!(
                    "{name} Codex mutation step uses unsupported pinned action input {key}."
                ));
            }
        }
    }
    for (key, expected) in [
        (
            "prompt-file",
            "${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.md",
        ),
        (
            "output-schema-file",
            "${{ runner.temp }}/dependency-remediation-trusted/dependency-remediation.json",
        ),
    ] {
        if inputs.get(key).and_then(YamlValue::as_str) != Some(expected) {
            errors.push(format!(
                "{name} Codex mutation step must use trusted absolute {key} {expected}."
            ));
        }
    }
    if inputs
        .get("codex-args")
        .and_then(YamlValue::as_str)
        .map(str::trim)
        != Some("[\"--profile\",\"ci_mutation\"]")
    {
        errors.push(format!(
            "{name} Codex args must select only the trusted ci_mutation profile."
        ));
    }
    if inputs.get("allow-bots").is_some()
        || inputs.get("allow-bot-users").and_then(YamlValue::as_str) != Some("dependabot[bot]")
    {
        errors.push(format!(
            "{name} must trust Dependabot through allow-bot-users, never Boolean allow-bots."
        ));
    }
}

fn validate_docs_taxonomy_artifacts(steps: &[WorkflowStepView], errors: &mut Vec<String>) {
    let name = "docs-taxonomy.yml";
    let preparations = steps
        .iter()
        .filter(|step| {
            step.run
                .contains(".artifacts/docs-taxonomy/run-context.json")
        })
        .collect::<Vec<_>>();
    let acquisitions = steps
        .iter()
        .filter(|step| step.run.trim() == PREPARE_COMMAND)
        .collect::<Vec<_>>();
    if preparations.len() != 1 || acquisitions.len() != 1 {
        errors.push(format!(
            "{name} must write exactly one non-secret run-context diagnostic before runtime acquisition."
        ));
        return;
    }

    let preparation = preparations[0];
    let acquisition = acquisitions[0];
    if preparation.job != acquisition.job || preparation.ordinal >= acquisition.ordinal {
        errors.push(format!(
            "{name} must create its artifact roots and diagnostic before runtime acquisition."
        ));
    }
    for required in [
        "mkdir -p .artifacts/codex .artifacts/docs-taxonomy",
        "jq -n",
        "> .artifacts/docs-taxonomy/run-context.json",
    ] {
        if !preparation.run.contains(required) {
            errors.push(format!(
                "{name} artifact preparation must include {required}."
            ));
        }
    }
    if preparation.raw.get("if").and_then(YamlValue::as_str)
        != Some("steps.codex_preflight.outputs.enabled == 'true'")
    {
        errors.push(format!(
            "{name} artifact preparation must use the Codex credential preflight gate."
        ));
    }
    if yaml_contains(&preparation.raw, "secrets.")
        || yaml_contains(&preparation.raw, "AGENT_PLUGINS_READ_TOKEN")
        || yaml_contains(&preparation.raw, "OPENAI_API_KEY")
    {
        errors.push(format!(
            "{name} run-context preparation must not receive or serialize credentials."
        ));
    }
}

fn validate_execution_state_trust(
    text: &str,
    payload: &YamlValue,
    steps: &[WorkflowStepView],
    errors: &mut Vec<String>,
) {
    let name = "execution_state_sync.yml";
    if !text.contains("\n  pull_request_target:\n") || text.contains("\n  pull_request:\n") {
        errors.push(format!(
            "{name} must use only the trusted pull_request_target event for automatic PR sync."
        ));
    }
    if !text.contains(
        "github.event_name != 'pull_request_target' || github.event.pull_request.head.repo.full_name == github.repository",
    ) {
        errors.push(format!(
            "{name} must gate pull_request_target runtime acquisition to same-repository pull requests."
        ));
    }
    if !text.contains("github.event.pull_request.head.repo.full_name == github.repository") {
        errors.push(format!(
            "{name} must reject fork pull requests before private runtime acquisition."
        ));
    }
    if text.contains("pull_request.head.sha") {
        errors.push(format!(
            "{name} must not check out pull-request head content before private runtime use."
        ));
    }
    let checkouts = steps
        .iter()
        .filter(|step| step.uses.starts_with("actions/checkout@"))
        .collect::<Vec<_>>();
    let expected_checkout_ref = "${{ github.event_name == 'pull_request_target' && github.event.action != 'closed' && github.event.pull_request.base.sha || github.sha }}";
    if checkouts.len() != 1 || checkouts[0].checkout_ref != expected_checkout_ref {
        errors.push(format!(
            "{name} must use exactly one checkout selecting the trusted pull-request base revision for active PR events and the event's current base revision for closed events."
        ));
    } else if checkouts[0].persist_credentials != Some(false) {
        errors.push(format!(
            "{name} trusted checkout must disable persisted GitHub credentials."
        ));
    }

    if text.contains("ROADMAP_GH_TOKEN:") || text.contains("${{ env.ROADMAP_GH_TOKEN }}") {
        errors.push(format!(
            "{name} must not retain the project PAT in job-level or aliased environment."
        ));
    }
    let expected_source =
        "${{ secrets.GH_PROJECTS_TOKEN != '' && 'GH_PROJECTS_TOKEN' || 'github.token' }}";
    let sync_job_env = payload
        .get("jobs")
        .and_then(|jobs| jobs.get("sync"))
        .and_then(|job| job.get("env"))
        .and_then(YamlValue::as_mapping);
    let source_key = YamlValue::String("ROADMAP_GH_TOKEN_SOURCE".into());
    if !sync_job_env.is_some_and(|env| {
        env.len() == 1 && env.get(&source_key).and_then(YamlValue::as_str) == Some(expected_source)
    }) {
        errors.push(format!(
            "{name} sync job environment must contain only the non-secret ROADMAP_GH_TOKEN_SOURCE label."
        ));
    }
    let root_exposes_project_token = payload.get("env").is_some_and(|env| {
        yaml_mapping_has_key(env, "GH_TOKEN")
            || yaml_mapping_has_key(env, "ROADMAP_GH_TOKEN")
            || yaml_contains(env, "secrets.GH_PROJECTS_TOKEN")
    });
    let another_job_exposes_project_token = payload
        .get("jobs")
        .and_then(YamlValue::as_mapping)
        .is_some_and(|jobs| {
            jobs.iter().any(|(job_name, job)| {
                job_name.as_str() != Some("sync")
                    && job.get("env").is_some_and(|env| {
                        yaml_mapping_has_key(env, "GH_TOKEN")
                            || yaml_mapping_has_key(env, "ROADMAP_GH_TOKEN")
                            || yaml_contains(env, "secrets.GH_PROJECTS_TOKEN")
                    })
            })
        });
    if root_exposes_project_token || another_job_exposes_project_token {
        errors.push(format!(
            "{name} must keep project credentials out of workflow- and job-level environment."
        ));
    }

    let expected_token =
        "${{ secrets.GH_PROJECTS_TOKEN != '' && secrets.GH_PROJECTS_TOKEN || github.token }}";
    for command in [PROJECT_SNAPSHOT_COMMAND, EXECUTION_STATE_COMMAND] {
        let matching = steps
            .iter()
            .filter(|step| step.run.contains(command))
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            continue;
        }
        let Some(env) = matching[0].raw.get("env").and_then(YamlValue::as_mapping) else {
            errors.push(format!(
                "{name} {command} step must receive a step-scoped GH_TOKEN."
            ));
            continue;
        };
        let gh_token_key = YamlValue::String("GH_TOKEN".into());
        if env.get(&gh_token_key).and_then(YamlValue::as_str) != Some(expected_token)
            || env.len() != 1
        {
            errors.push(format!(
                "{name} {command} step must expose only the direct project-token GH_TOKEN."
            ));
        }
    }

    for step in steps.iter().filter(|step| {
        !step.run.contains(PROJECT_SNAPSHOT_COMMAND) && !step.run.contains(EXECUTION_STATE_COMMAND)
    }) {
        if step.raw.get("env").is_some_and(|env| {
            yaml_mapping_has_key(env, "GH_TOKEN")
                || yaml_mapping_has_key(env, "ROADMAP_GH_TOKEN")
                || yaml_contains(env, "secrets.GH_PROJECTS_TOKEN")
        }) {
            errors.push(format!(
                "{name} must expose the project token only to publisher GitHub operation steps."
            ));
        }
    }
}

fn validate_execution_state_artifacts(steps: &[WorkflowStepView], errors: &mut Vec<String>) {
    let name = "execution_state_sync.yml";
    let preparations = steps
        .iter()
        .filter(|step| {
            step.run.contains(".artifacts/execution-state/")
                && step.run.contains("run-context.json")
        })
        .collect::<Vec<_>>();
    let acquisitions = steps
        .iter()
        .filter(|step| step.run.trim() == PREPARE_COMMAND)
        .collect::<Vec<_>>();
    if preparations.len() != 1 || acquisitions.len() != 1 {
        errors.push(format!(
            "{name} must write exactly one non-secret run-context diagnostic before runtime acquisition."
        ));
        return;
    }

    let preparation = preparations[0];
    let acquisition = acquisitions[0];
    if preparation.job != acquisition.job || preparation.ordinal >= acquisition.ordinal {
        errors.push(format!(
            "{name} must create its artifact root and diagnostic before runtime acquisition."
        ));
    }
    if preparation.raw.get("id").and_then(YamlValue::as_str) != Some("artifact-root")
        || !preparation
            .run
            .contains("echo \"path=$root\" >> \"$GITHUB_OUTPUT\"")
    {
        errors.push(format!(
            "{name} run-context preparation must publish the artifact-root path output."
        ));
    }
    if yaml_contains(&preparation.raw, "secrets.")
        || yaml_contains(&preparation.raw, "AGENT_PLUGINS_READ_TOKEN")
        || yaml_contains(&preparation.raw, "GH_PROJECTS_TOKEN")
    {
        errors.push(format!(
            "{name} run-context preparation must not receive or serialize credentials."
        ));
    }

    let uploads = steps
        .iter()
        .filter(|step| step.uses.starts_with("actions/upload-artifact@"))
        .collect::<Vec<_>>();
    if uploads.len() != 1 {
        errors.push(format!(
            "{name} must define exactly one bounded artifact upload."
        ));
        return;
    }
    let upload = uploads[0];
    let expected_if = "${{ (failure() || github.event_name == 'workflow_dispatch') && steps.artifact-root.outputs.path != '' }}";
    if upload.raw.get("if").and_then(YamlValue::as_str) != Some(expected_if) {
        errors.push(format!(
            "{name} artifact upload must be failure/manual-only and require a prepared path."
        ));
    }
    let Some(with) = upload.raw.get("with").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} artifact upload must define its bounded upload inputs."
        ));
        return;
    };
    for (key, expected) in [
        ("path", "${{ steps.artifact-root.outputs.path }}"),
        ("if-no-files-found", "error"),
    ] {
        if with
            .get(YamlValue::String(key.into()))
            .and_then(YamlValue::as_str)
            != Some(expected)
        {
            errors.push(format!(
                "{name} artifact upload must set {key} to {expected}."
            ));
        }
    }
    if with
        .get(YamlValue::String("include-hidden-files".into()))
        .and_then(YamlValue::as_bool)
        != Some(true)
        || with
            .get(YamlValue::String("retention-days".into()))
            .and_then(YamlValue::as_u64)
            != Some(14)
    {
        errors.push(format!(
            "{name} artifact upload must include hidden files and retain diagnostics for 14 days."
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
