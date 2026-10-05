use std::path::Path;

use serde_yaml::Value as YamlValue;

use super::{WORKFLOWS, read_text};

mod codex_action;

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
const PROJECT_SNAPSHOT: &str = "gh steward snapshot project";
const EXECUTION_PREPARE: &str = "gh steward execution prepare";
const EXECUTION_APPLY: &str = "gh steward execution apply";
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
            ("gpt-6-luna", "use the qualified lightweight Codex model"),
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

    if let Some(source) = read_text(
        repo_root,
        "xtask/src/dependency_remediation/candidate.rs",
        errors,
    ) {
        validate_dependency_candidate_artifact_ids(&source, errors);
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
    if kind == AgentPluginWorkflowKind::CodexPlugins && !text.contains(CODEX_PLUGIN_SETUP) {
        errors.push(format!(
            "{name} must install the selected public plugins into isolated Codex state."
        ));
    }
    if kind == AgentPluginWorkflowKind::CodexPlugins && !text.contains("gpt-6-luna") {
        errors.push(format!("{name} must use the qualified gpt-6-luna model."));
    }
    if kind == AgentPluginWorkflowKind::ExecutionState
        && [PROJECT_SNAPSHOT, EXECUTION_PREPARE, EXECUTION_APPLY]
            .iter()
            .any(|command| !text.contains(command))
    {
        errors.push(format!(
            "{name} must use gh steward for Project snapshots and reviewed execution prepare/apply."
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
    codex_action::validate_args(&steps, name, errors);

    match name {
        "dependency-remediation.yml" => {
            validate_dependency_remediation_trust(text, &payload, &steps, errors)
        }
        "execution_state_sync.yml" => {
            validate_execution_state_trust(text, &payload, &steps, errors);
            validate_execution_state_artifacts(text, &steps, errors);
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
    for step in prepares
        .iter()
        .chain(verifies.iter())
        .chain(plugin_setup.iter())
    {
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
                "{name} acquisition and isolated plugin setup must not receive GitHub tokens or other workflow secrets."
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
                step.run.contains("gh steward ")
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
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} must isolate proposal, candidate verification, and trusted publication jobs."
        ));
        return;
    };
    let job = |job_name: &str| jobs.get(YamlValue::String(job_name.to_owned()));
    for job_name in ["capture", "proposal", "verify-candidate", "publish"] {
        let Some(job) = job(job_name) else {
            errors.push(format!("{name} must define the {job_name} stage."));
            return;
        };
        let permissions = job.get("permissions").and_then(YamlValue::as_mapping);
        let expected: &[(&str, &str)] = match job_name {
            "capture" => &[("contents", "read")],
            "proposal" | "verify-candidate" => &[("actions", "read"), ("contents", "read")],
            "publish" => &[
                ("actions", "read"),
                ("contents", "write"),
                ("pull-requests", "write"),
            ],
            _ => unreachable!(),
        };
        if permissions.is_none() {
            errors.push(format!(
                "{name} {job_name} job must declare least-privilege permissions."
            ));
            continue;
        }
        let permissions = permissions.unwrap();
        for (scope, expected_value) in expected {
            if permissions
                .get(YamlValue::String((*scope).to_owned()))
                .and_then(YamlValue::as_str)
                != Some(*expected_value)
            {
                errors.push(format!(
                    "{name} {job_name} job must grant {scope}: {expected_value}."
                ));
            }
        }
        if job.get("env").is_some_and(|env| {
            yaml_mapping_has_key(env, "GH_TOKEN")
                || yaml_mapping_has_key(env, "GITHUB_TOKEN")
                || yaml_contains(env, "secrets.")
        }) {
            errors.push(format!(
                "{name} {job_name} job must not expose credentials through job environment."
            ));
        }
        if job_name != "publish"
            && permissions
                .values()
                .any(|permission| matches!(permission.as_str(), Some("write" | "admin")))
        {
            errors.push(format!("{name} {job_name} job must remain read-only."));
        }
    }

    let steps_for = |job_name: &str| {
        steps
            .iter()
            .filter(|step| step.job == job_name)
            .collect::<Vec<_>>()
    };
    let capture_steps = steps_for("capture");
    let proposal_steps = steps_for("proposal");
    let verify_steps = steps_for("verify-candidate");
    let publish_steps = steps_for("publish");
    let trusted_ref = concat!("$", "{{ github.workflow_sha }}");
    let trusted_only = |job_steps: &[&WorkflowStepView]| {
        job_steps
            .iter()
            .filter(|step| step.uses.starts_with("actions/checkout@"))
            .all(|step| step.checkout_ref == trusted_ref && step.persist_credentials == Some(false))
    };
    let has_trusted_checkout = |job_steps: &[&WorkflowStepView]| {
        job_steps.iter().any(|step| {
            step.uses.starts_with("actions/checkout@")
                && step.checkout_ref == trusted_ref
                && step.persist_credentials == Some(false)
        })
    };
    let credentials_disabled = |job_steps: &[&WorkflowStepView]| {
        job_steps
            .iter()
            .filter(|step| step.uses.starts_with("actions/checkout@"))
            .all(|step| step.persist_credentials == Some(false))
    };

    if !trusted_only(&proposal_steps)
        || proposal_steps
            .iter()
            .filter(|step| step.uses.starts_with("actions/checkout@"))
            .count()
            != 1
    {
        errors.push(format!(
            "{name} proposal must use only the exact trusted workflow checkout with credentials disabled."
        ));
    }
    let codex = proposal_steps.iter().find(|step| step.uses == CODEX_ACTION);
    if codex.is_none_or(|step| {
        step.raw.get("env").is_some_and(|env| {
            yaml_mapping_has_key(env, "GH_TOKEN")
                || yaml_mapping_has_key(env, "GITHUB_TOKEN")
                || yaml_contains(env, "secrets.GITHUB_TOKEN")
        })
    }) {
        errors.push(format!(
            "{name} bounded Codex proposal must run without a GitHub token."
        ));
    }
    if proposal_steps.iter().any(|step| {
        step.uses.starts_with("actions/checkout@")
            && (step.checkout_ref.contains("pull_request.head")
                || step.checkout_ref == concat!("$", "{{ github.sha }}"))
    }) || proposal_steps.iter().any(|step| {
        step.run.contains("cargo test")
            || step.run.contains("dependency-remediation-publish.sh")
            || step.run.contains("working-directory: source")
    }) {
        errors.push(format!(
            "{name} Codex proposal must not check out or execute dependency-head code."
        ));
    }
    for required in [
        "cp .github/codex/prompts/dependency-remediation.md",
        "cp .github/codex/schemas/dependency-remediation.json",
        "scripts/prepare-codex-plugins.sh",
        "source.patch",
        "source-verification.json",
    ] {
        if !text.contains(required) {
            errors.push(format!(
                "{name} must provide bounded trusted proposal input {required}."
            ));
        }
    }

    let source_ref = concat!("$", "{{ steps.source.outputs.source_sha }}");
    let candidate_ref = concat!("$", "{{ steps.validated_source.outputs.source_sha }}");
    let source_checkout = capture_steps.iter().any(|step| {
        step.uses.starts_with("actions/checkout@")
            && step.checkout_ref == source_ref
            && step.persist_credentials == Some(false)
            && step
                .raw
                .get("with")
                .is_some_and(|with| with.get("path").and_then(YamlValue::as_str) == Some("source"))
    });
    let candidate_checkout = verify_steps.iter().any(|step| {
        step.uses.starts_with("actions/checkout@")
            && step.checkout_ref == candidate_ref
            && step.persist_credentials == Some(false)
            && step.raw.get("with").is_some_and(|with| {
                with.get("path").and_then(YamlValue::as_str) == Some("candidate")
            })
    });
    if !has_trusted_checkout(&capture_steps)
        || !source_checkout
        || !credentials_disabled(&capture_steps)
        || !has_trusted_checkout(&verify_steps)
        || !candidate_checkout
        || !credentials_disabled(&verify_steps)
    {
        errors.push(format!(
            "{name} capture and candidate verification must use exact event-bound source checkouts without persisted credentials."
        ));
    }
    if !capture_steps.iter().any(|step| {
        step.run
            .contains("env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY cargo test")
    }) || !verify_steps.iter().any(|step| {
        step.run
            .contains("env -u GH_TOKEN -u GITHUB_TOKEN -u OPENAI_API_KEY cargo test")
    }) {
        errors.push(format!(
            "{name} must test source and the complete candidate in credential-free jobs."
        ));
    }
    let trusted_xtask = verify_steps.iter().find(|step| {
        step.run.contains(
            "cargo build --manifest-path \"$GITHUB_WORKSPACE/xtask/Cargo.toml\" --locked --release",
        ) && step
            .run
            .contains("GIT_SLOP_XTASK_BIN=%s/release/git-slop-xtask")
    });
    let candidate_apply = verify_steps.iter().find(|step| {
        step.run.contains("$GIT_SLOP_XTASK_BIN")
            && step.run.contains("dependency-remediation apply-candidate")
    });
    let candidate_check = verify_steps.iter().find(|step| {
        step.run.contains("$GIT_SLOP_XTASK_BIN")
            && step
                .run
                .contains("dependency-remediation verify-candidate \"$SOURCE_SHA\"")
            && step.run.contains("candidate_tree_sha")
    });
    if trusted_xtask.is_none_or(|build| {
        candidate_apply.is_none_or(|apply| build.ordinal >= apply.ordinal)
            || candidate_check.is_none_or(|check| build.ordinal >= check.ordinal)
    }) || candidate_apply.is_none()
        || candidate_check.is_none()
    {
        errors.push(format!(
            "{name} must use a prebuilt trusted xtask to apply and recheck the exact source-bound candidate tree."
        ));
    }

    let recovery = publish_steps
        .iter()
        .find(|step| step.run.contains("runs acquire-handoff"));
    let candidate_acquire = publish_steps
        .iter()
        .find(|step| step.run.contains("runs acquire-publication-candidate"));
    let context_start = publish_steps
        .iter()
        .find(|step| step.run.contains("runs context-start"));
    let prepare_intent = publish_steps.iter().find(|step| {
        step.run.contains("dependency-remediation-publish.sh") && step.run.contains(" prepare ")
    });
    let verify_publication = publish_steps
        .iter()
        .find(|step| step.run.contains("runs verify-publication"));
    let publish_write = publish_steps.iter().find(|step| {
        step.run.contains("dependency-remediation-publish.sh") && step.run.contains(" continue ")
    });
    let terminal_upload = publish_steps
        .iter()
        .find(|step| step.uses.starts_with("actions/upload-artifact@"));
    let finalizer = publish_steps.iter().find(|step| {
        step.run.contains("finalize-gh-steward-run.sh") && step.run.contains("args=(--workflow")
    });
    let (
        Some(recovery),
        Some(candidate_acquire),
        Some(context_start),
        Some(prepare_intent),
        Some(verify_publication),
        Some(publish_write),
        Some(terminal_upload),
        Some(finalizer),
    ) = (
        recovery,
        candidate_acquire,
        context_start,
        prepare_intent,
        verify_publication,
        publish_write,
        terminal_upload,
        finalizer,
    )
    else {
        errors.push(format!(
            "{name} publisher must acquire recovery and candidate evidence, create a native context, verify before writes, and finalize its terminal artifact."
        ));
        return;
    };
    if !trusted_only(&publish_steps)
        || publish_steps
            .iter()
            .filter(|step| step.uses.starts_with("actions/checkout@"))
            .count()
            != 1
    {
        errors.push(format!(
            "{name} publisher must use only one exact trusted checkout with credentials disabled."
        ));
    }
    if !(recovery.ordinal < candidate_acquire.ordinal
        && candidate_acquire.ordinal < context_start.ordinal
        && context_start.ordinal < prepare_intent.ordinal
        && prepare_intent.ordinal < verify_publication.ordinal
        && verify_publication.ordinal < publish_write.ordinal
        && publish_write.ordinal < terminal_upload.ordinal
        && terminal_upload.ordinal < finalizer.ordinal)
    {
        errors.push(format!(
            "{name} publisher step order must be recovery < candidate < context < intent < verify < write < artifact < finalize; observed {:?}.",
            [recovery.ordinal, candidate_acquire.ordinal, context_start.ordinal, prepare_intent.ordinal,
                verify_publication.ordinal, publish_write.ordinal, terminal_upload.ordinal, finalizer.ordinal]
        ));
    }
    if !candidate_acquire
        .run
        .contains("--workflow-sha \"$GITHUB_WORKFLOW_SHA\"")
        || !candidate_acquire
            .run
            .contains("--artifact-id \"$CANDIDATE_ARTIFACT_ID\"")
        || !candidate_acquire
            .run
            .contains("--artifact-digest \"$CANDIDATE_ARTIFACT_DIGEST\"")
        || !verify_publication
            .run
            .contains("--workflow-sha \"$GITHUB_WORKFLOW_SHA\"")
        || !verify_publication
            .run
            .contains("--repo-root \"$GH_STEWARD_TRUSTED_ROOT\"")
        || !publish_steps.iter().any(|step| {
            step.run
                .contains("git worktree add --detach \"$control\" \"$GITHUB_WORKFLOW_SHA\"")
                && step.run.contains("GH_STEWARD_TRUSTED_ROOT")
        })
    {
        errors.push(format!(
            "{name} must bind candidate acquisition and qualification to the trusted workflow SHA and control checkout."
        ));
    }
    if job("publish").is_none_or(|publish| {
        publish
            .get("needs")
            .is_none_or(|needs| !yaml_contains(needs, "verify-candidate"))
    }) || !text.contains("needs.verify-candidate.outputs.artifact_id")
        || !text.contains("needs.verify-candidate.outputs.artifact_digest")
    {
        errors.push(format!(
            "{name} trusted publisher must bind the exact immutable verified-candidate artifact ID and digest."
        ));
    }
    if publish_steps.iter().any(|step| {
        step.uses == CODEX_ACTION
            || step.run.contains("cargo test")
            || step.run.contains("working-directory: candidate")
    }) {
        errors.push(format!(
            "{name} publisher must not run Codex or execute candidate source code with write permissions."
        ));
    }
    for required in [
        "publication/candidate.json",
        "publication/patch.diff",
        "publication/result.json",
        "events/trigger-event.json",
        "candidate_tree_sha",
    ] {
        if !text.contains(required) {
            errors.push(format!(
                "{name} must bind exact candidate files and credential-free verification evidence."
            ));
            break;
        }
    }
    if !verify_steps.iter().any(|step| {
        step.run
            .contains("dependency-remediation create-candidate-evidence")
    }) {
        errors.push(format!(
            "{name} candidate evidence must be created by the trusted native adapter."
        ));
    }
}

pub(super) fn validate_dependency_candidate_artifact_ids(source: &str, errors: &mut Vec<String>) {
    for required in [
        "let id = positive_env(&format!(\"{prefix}_ID\"))?;",
        "Ok(json!({\"id\":id,\"name\":name,\"digest\":digest}))",
    ] {
        if !source.contains(required) {
            errors.push(
                "dependency-remediation candidate evidence must preserve upstream artifact IDs as positive JSON integers."
                    .to_owned(),
            );
            return;
        }
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
            "{name} must use pull_request_target for automatic PR synchronization."
        ));
    }
    if !text.contains("github.event.pull_request.head.repo.full_name == github.repository")
        || !text.contains("github.event_name != 'pull_request_target' ||")
    {
        errors.push(format!(
            "{name} must reject fork pull requests before acquisition."
        ));
    }
    if !text.contains("cancel-in-progress: false") {
        errors.push(format!("{name} must not cancel an in-flight mutation run."));
    }
    if !text.contains("run-name: Execution State Sync") {
        errors.push(format!(
            "{name} must keep a stable run name for exact recovery identity."
        ));
    }
    let trusted_ref = concat!("$", "{{ github.workflow_sha }}");
    let checkouts = steps
        .iter()
        .filter(|step| step.uses.starts_with("actions/checkout@"))
        .collect::<Vec<_>>();
    if checkouts.len() < 3
        || checkouts
            .iter()
            .any(|step| step.checkout_ref != trusted_ref || step.persist_credentials != Some(false))
    {
        errors.push(format!("{name} must check out only the exact trusted workflow source with credentials disabled."));
    }
    if payload.get("env").is_some_and(|env| {
        yaml_mapping_has_key(env, "GH_TOKEN")
            || yaml_mapping_has_key(env, "GITHUB_TOKEN")
            || yaml_contains(env, "secrets.")
    }) {
        errors.push(format!(
            "{name} must not expose credentials at workflow scope."
        ));
    }
    let Some(jobs) = payload.get("jobs").and_then(YamlValue::as_mapping) else {
        errors.push(format!(
            "{name} must isolate preparation from native application."
        ));
        return;
    };
    let prepare_job = jobs.get(YamlValue::String("prepare".into()));
    let apply_job = jobs.get(YamlValue::String("apply".into()));
    let (Some(prepare_job), Some(apply_job)) = (prepare_job, apply_job) else {
        errors.push(format!(
            "{name} must isolate preparation from native application."
        ));
        return;
    };
    for (job_name, job) in [("prepare", prepare_job), ("apply", apply_job)] {
        if job.get("env").is_some_and(|env| {
            yaml_mapping_has_key(env, "GH_TOKEN")
                || yaml_mapping_has_key(env, "GITHUB_TOKEN")
                || yaml_contains(env, "secrets.")
        }) {
            errors.push(format!(
                "{name} {job_name} job must not expose credentials through job environment."
            ));
        }
    }
    let prepare_permissions = prepare_job
        .get("permissions")
        .and_then(YamlValue::as_mapping);
    if prepare_permissions.is_none_or(|permissions| {
        permissions
            .values()
            .any(|value| matches!(value.as_str(), Some("write" | "admin")))
    }) {
        errors.push(format!("{name} prepare job must remain read-only."));
    }
    for (scope, expected) in [("contents", "read"), ("actions", "read")] {
        if prepare_permissions
            .and_then(|permissions| permissions.get(YamlValue::String(scope.into())))
            .and_then(YamlValue::as_str)
            != Some(expected)
        {
            errors.push(format!("{name} prepare job must have {scope}: {expected}."));
        }
    }
    for command in [PROJECT_SNAPSHOT, EXECUTION_PREPARE, EXECUTION_APPLY] {
        let matching = steps
            .iter()
            .filter(|step| step.run.contains(command))
            .collect::<Vec<_>>();
        let expected_job = if command == EXECUTION_APPLY {
            "apply"
        } else {
            "prepare"
        };
        if matching.len() != 1 || matching[0].job != expected_job {
            errors.push(format!(
                "{name} must define exactly one {command} operation in the {expected_job} job."
            ));
            continue;
        }
        validate_step_token(matching[0], PROJECT_TOKEN, name, errors);
    }
    let recovery = steps
        .iter()
        .find(|step| step.job == "prepare" && step.run.contains("runs recover"));
    if let Some(step) = recovery {
        validate_step_token(step, concat!("$", "{{ github.token }}"), name, errors);
    } else {
        errors.push(format!(
            "{name} must read prior workflow history before planning."
        ));
    }
    let apply_handoff = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains("runs acquire-handoff"));
    if let Some(step) = apply_handoff {
        validate_step_token(step, concat!("$", "{{ github.token }}"), name, errors);
    } else {
        errors.push(format!(
            "{name} apply job must acquire an immutable native handoff."
        ));
    }
    if steps.iter().any(|step| {
        (step.job == "prepare" || step.job == "apply")
            && (yaml_mapping_has_key(&step.raw, "GH_TOKEN")
                || yaml_contains(&step.raw, "AGENT_PLUGINS_READ_TOKEN")
                || yaml_contains(&step.raw, "AGENT_PLUGINS_GIT_TOKEN"))
            && !step.run.contains(PROJECT_SNAPSHOT)
            && !step.run.contains(EXECUTION_PREPARE)
            && !step.run.contains(EXECUTION_APPLY)
            && !step.run.contains("runs recover")
            && !step.run.contains("runs acquire-handoff")
            && !step.run.contains("finalize-gh-steward-run")
    }) {
        errors.push(format!("{name} must expose GitHub tokens only to scoped recovery, handoff, Project, or native execution steps."));
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

fn validate_execution_state_artifacts(
    text: &str,
    steps: &[WorkflowStepView],
    errors: &mut Vec<String>,
) {
    let name = "execution_state_sync.yml";
    let target = steps.iter().find(|step| {
        step.job == "prepare"
            && step.get_name() == Some("Resolve exact target and capture event after recovery")
    });
    let recovery = steps
        .iter()
        .find(|step| step.job == "prepare" && step.run.contains("runs recover"));
    let fail_closed = steps
        .iter()
        .find(|step| step.get_name() == Some("Stop when prior mutation evidence is unavailable"));
    let preparation = steps
        .iter()
        .find(|step| step.job == "prepare" && step.run.contains(EXECUTION_PREPARE));
    let prepare_upload = steps
        .iter()
        .find(|step| step.job == "prepare" && step.uses.starts_with("actions/upload-artifact@"));
    let apply_handoff = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains("runs acquire-handoff"));
    let install_journal = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains("runs context-install-journal"));
    let dispatch_intent = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains("phase=\"dispatching\""));
    let apply = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains(EXECUTION_APPLY));
    let capture_journal = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains("runs context-capture-journal"));
    let mark_completed = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains("runs context-mark-plan"));
    let apply_uploads = steps
        .iter()
        .filter(|step| step.job == "apply" && step.uses.starts_with("actions/upload-artifact@"))
        .collect::<Vec<_>>();
    let finalizer = steps
        .iter()
        .find(|step| step.job == "apply" && step.run.contains("finalize-gh-steward-run"));
    let noop_acquire = steps.iter().find(|step| {
        step.job == "settle_noop"
            && step.run.contains("runs acquire-handoff")
            && step.run.contains("--purpose transport")
    });
    let noop_finish = steps
        .iter()
        .find(|step| step.job == "settle_noop" && step.run.contains("runs finish-noop"));
    let noop_finalizer = steps
        .iter()
        .find(|step| step.job == "settle_noop" && step.run.contains("finalize-gh-steward-run"));
    let (
        Some(target),
        Some(recovery),
        Some(fail_closed),
        Some(preparation),
        Some(prepare_upload),
        Some(apply_handoff),
        Some(install_journal),
        Some(dispatch_intent),
        Some(apply),
        Some(capture_journal),
        Some(mark_completed),
        Some(finalizer),
        Some(noop_acquire),
        Some(noop_finish),
        Some(noop_finalizer),
    ) = (
        target,
        recovery,
        fail_closed,
        preparation,
        prepare_upload,
        apply_handoff,
        install_journal,
        dispatch_intent,
        apply,
        capture_journal,
        mark_completed,
        finalizer,
        noop_acquire,
        noop_finish,
        noop_finalizer,
    )
    else {
        errors.push(format!("{name} must retain native target, recovery, plan, handoff, journal, apply, and no-op settlement steps."));
        return;
    };
    if apply_uploads.len() != 2 {
        errors.push(format!(
            "{name} must upload one terminal package and one settlement checkpoint."
        ));
        return;
    }
    let terminal_upload = apply_uploads[0];
    let checkpoint_upload = apply_uploads[1];
    if !(recovery.ordinal < target.ordinal && target.ordinal < preparation.ordinal)
        || !(apply_handoff.ordinal < install_journal.ordinal
            && install_journal.ordinal < dispatch_intent.ordinal
            && dispatch_intent.ordinal < apply.ordinal
            && apply.ordinal < capture_journal.ordinal
            && capture_journal.ordinal == mark_completed.ordinal
            && mark_completed.ordinal < terminal_upload.ordinal
            && terminal_upload.ordinal < finalizer.ordinal
            && finalizer.ordinal < checkpoint_upload.ordinal)
    {
        errors.push(format!("{name} must recover before prepare and capture durable journal evidence before finalization."));
    }
    if !fail_closed
        .raw
        .get("if")
        .and_then(YamlValue::as_str)
        .is_some_and(|condition| {
            condition.contains("steps.recovery.outputs.outcome == 'recovery_needed'")
        })
        || !fail_closed.run.contains("exit 1")
    {
        errors.push(format!(
            "{name} must stop when prior plan/journal recovery is uncertain."
        ));
    }
    if !preparation.run.contains("runs context-start")
        || !preparation.run.contains("runs context-record-plan")
    {
        errors.push(format!(
            "{name} must register the exact execution plan through native context commands."
        ));
    }
    let handoff_with = prepare_upload.raw.get("with");
    let prepared_package = concat!("$", "{{ steps.package_state.outputs.package }}");
    if prepare_upload.raw.get("id").and_then(YamlValue::as_str) != Some("upload_prepared")
        || handoff_with.is_none_or(|with| {
            !with
                .get("name")
                .and_then(YamlValue::as_str)
                .is_some_and(|name| name.ends_with("-handoff-00"))
                || with.get("path").and_then(YamlValue::as_str) != Some(prepared_package)
                || with
                    .get("include-hidden-files")
                    .and_then(YamlValue::as_bool)
                    != Some(true)
                || with.get("retention-days").and_then(YamlValue::as_i64) != Some(14)
        })
    {
        errors.push(format!(
            "{name} must upload a bounded immutable prepared handoff from RUNNER_TEMP."
        ));
    }
    if !apply_handoff
        .raw
        .get("env")
        .is_some_and(|env| yaml_contains(env, "needs.prepare.outputs.artifact_id"))
        || !apply_handoff
            .raw
            .get("env")
            .is_some_and(|env| yaml_contains(env, "needs.prepare.outputs.artifact_digest"))
        || !apply_handoff.run.contains("--artifact-id")
        || !apply_handoff.run.contains("--artifact-digest")
        || apply_handoff.run.contains("--purpose transport")
        || steps
            .iter()
            .any(|step| step.job == "apply" && step.uses.starts_with("actions/download-artifact@"))
    {
        errors.push(format!("{name} apply job must acquire the exact immutable handoff ID and digest with apply purpose."));
    }
    if !install_journal
        .raw
        .get("if")
        .and_then(YamlValue::as_str)
        .is_some_and(|condition| condition.contains("needs.prepare.outputs.mode == 'resumed'"))
        || !install_journal.run.contains("recovery-source.json")
        || !install_journal.run.contains("context-install-journal")
    {
        errors.push(format!("{name} may install a journal only for a resumed package with its exact recovery source."));
    }
    if !dispatch_intent.run.contains("dispatching")
        || !apply.run.contains("--approve-plan-sha")
        || !apply.run.contains("apply-results/execution.json")
    {
        errors.push(format!(
            "{name} must persist dispatch intent before applying the exact recorded plan."
        ));
    }
    if !capture_journal.run.contains("context-capture-journal")
        || !capture_journal.run.contains("journal-root")
        || !mark_completed
            .run
            .contains("--name execution --status completed")
        || !mark_completed.run.contains("apply-results/execution.json")
    {
        errors.push(format!("{name} must use native monotonic journal capture and mark completion only from the exact apply result."));
    }
    let terminal_package = concat!("$", "{{ runner.temp }}/execution-state-package");
    if !terminal_upload.raw.get("with").is_some_and(|with| {
        with.get("name")
            .and_then(YamlValue::as_str)
            .is_some_and(|name| name.contains("needs.prepare.outputs.artifact_name"))
            && with.get("path").and_then(YamlValue::as_str) == Some(terminal_package)
            && with.get("retention-days").and_then(YamlValue::as_i64) == Some(90)
    }) || !checkpoint_upload.raw.get("with").is_some_and(|with| {
        with.get("path")
            .and_then(YamlValue::as_str)
            .is_some_and(|path| path.contains("checkpoint_path"))
    }) || !text.contains("recovery_needed")
    {
        errors.push(format!(
            "{name} must retain run-scoped recovery and terminal settlement artifacts."
        ));
    }
    if !finalizer.run.contains("--artifact-id")
        || !finalizer.run.contains("--artifact-digest")
        || !noop_acquire.run.contains("--artifact-id")
        || !noop_acquire.run.contains("--artifact-digest")
        || !noop_finish.run.contains("--workflow-sha")
        || !noop_finalizer.run.contains("--artifact-digest")
        || !text.contains("needs.apply.result == 'skipped'")
    {
        errors.push(format!(
            "{name} must close failed or no-op attempts with exact artifacts and checkpoints."
        ));
    }
}
impl WorkflowStepView {
    fn get_name(&self) -> Option<&str> {
        self.raw.get("name").and_then(YamlValue::as_str)
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
