//! Exact source handoff selection and a new, separately reviewed apply context.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};

use super::{
    HANDOFF_SUFFIX, MUTATOR_STEP, PLAN_COMMAND, PLAN_NAME, WORKFLOW_FILE, append_outputs,
    canonical_target, create_private_dir, ensure_new, ensure_sha, ensure_workflow_sha, field,
    is_lower_hex, json_bytes, package_file, parse_canonical_positive, parse_json, positive_number,
    read_json, read_json_documents, read_package_json, sha256, string_field, validate_apply,
    validate_dispatch_inputs, write_new,
};

#[derive(Debug, Args)]
pub(super) struct ValidateApplyInputs {
    #[arg(long)]
    pub(super) operation: String,
    #[arg(long)]
    pub(super) pr_number: String,
    #[arg(long)]
    pub(super) issue_number: String,
    #[arg(long)]
    pub(super) plan_run_id: String,
    #[arg(long)]
    pub(super) approve_plan_sha: String,
    #[arg(long)]
    pub(super) event: PathBuf,
    #[arg(long)]
    pub(super) target: PathBuf,
    #[arg(long)]
    pub(super) github_output: PathBuf,
}

impl ValidateApplyInputs {
    pub(super) fn run(self) -> Result<()> {
        let recovery_key = validate_apply(
            self.operation,
            &self.pr_number,
            &self.issue_number,
            &self.plan_run_id,
            &self.approve_plan_sha,
            &self.event,
        )?;
        let target_bytes = canonical_target(&self.approve_plan_sha)?;
        ensure_new(&self.target)?;
        write_new(&self.target, &target_bytes)?;
        append_outputs(
            &self.github_output,
            &[
                ("recovery_key", recovery_key),
                ("target_path", self.target.display().to_string()),
            ],
        )?;
        Ok(())
    }
}

#[derive(Debug, Args)]
pub(super) struct SelectHandoff {
    #[arg(long)]
    pub(super) current_run: PathBuf,
    #[arg(long)]
    pub(super) selected_run: PathBuf,
    #[arg(long)]
    pub(super) artifact_pages: PathBuf,
    #[arg(long)]
    pub(super) repository: String,
    #[arg(long)]
    pub(super) current_run_id: String,
    #[arg(long)]
    pub(super) default_branch: String,
    #[arg(long)]
    pub(super) run_name: String,
    #[arg(long)]
    pub(super) plan_run_id: String,
    #[arg(long)]
    pub(super) output: PathBuf,
}

impl SelectHandoff {
    pub(super) fn run(self) -> Result<()> {
        let current_id = parse_canonical_positive(&self.current_run_id, "current workflow run ID")?;
        let plan_run_id = parse_canonical_positive(&self.plan_run_id, "plan_run_id")?;
        let current = read_json(&self.current_run)?;
        let selected = read_json(&self.selected_run)?;
        let current_workflow =
            positive_number(field(&current, "workflow_id")?, "current workflow ID")?;
        let current_repository_id = positive_number(
            field(field(&current, "repository")?, "id")?,
            "current repository ID",
        )?;
        let scope = RunScope {
            workflow_id: current_workflow,
            repository_id: current_repository_id,
            repository: &self.repository,
            default_branch: &self.default_branch,
            run_name: &self.run_name,
        };
        validate_run_identity(&current, current_id, &scope, false)?;
        let selected_workflow =
            positive_number(field(&selected, "workflow_id")?, "selected workflow ID")?;
        let selected_repository_id = positive_number(
            field(field(&selected, "repository")?, "id")?,
            "selected repository ID",
        )?;
        ensure!(
            selected_workflow == current_workflow,
            "selected run belongs to a different workflow ID"
        );
        ensure!(
            selected_repository_id == current_repository_id,
            "selected run belongs to a different repository ID"
        );
        let attempt = positive_number(
            field(&selected, "run_attempt")?,
            "selected workflow run attempt",
        )?;
        validate_run_identity(&selected, plan_run_id, &scope, true)?;
        let source_head_sha = string_field(&selected, "head_sha")?;
        ensure!(
            is_lower_hex(&source_head_sha, 40),
            "selected run head SHA is invalid"
        );
        let source_head_branch = string_field(&selected, "head_branch")?;
        let suffix = format!("-run-{plan_run_id}-attempt-{attempt}{HANDOFF_SUFFIX}");
        let pages = read_json_documents(&self.artifact_pages)?;
        let artifacts = pages
            .iter()
            .flat_map(|page| {
                page.get("artifacts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter(|artifact| {
                artifact
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.ends_with(&suffix))
            })
            .collect::<Vec<_>>();
        ensure!(
            artifacts.len() == 1,
            "selected run must have one unique handoff artifact for its latest attempt"
        );
        let artifact = artifacts[0];
        ensure!(
            field(artifact, "expired")?.as_bool() == Some(false),
            "selected handoff artifact is expired"
        );
        let digest = string_field(artifact, "digest")?;
        ensure!(
            digest
                .strip_prefix("sha256:")
                .is_some_and(|value| is_lower_hex(value, 64)),
            "selected artifact digest is invalid"
        );
        let artifact_id = positive_number(field(artifact, "id")?, "selected artifact ID")?;
        let workflow_run = field(artifact, "workflow_run")?;
        ensure!(
            positive_number(field(workflow_run, "id")?, "artifact source run ID")? == plan_run_id,
            "artifact is not attached to the selected source run"
        );
        ensure!(
            positive_number(
                field(workflow_run, "repository_id")?,
                "artifact source repository ID"
            )? == current_repository_id,
            "artifact is not attached to the selected repository"
        );
        ensure!(
            string_field(workflow_run, "head_sha")? == source_head_sha,
            "artifact source head SHA does not match the selected run"
        );
        ensure!(
            string_field(workflow_run, "head_branch")? == source_head_branch,
            "artifact source branch does not match the selected run"
        );
        ensure_new(&self.output)?;
        write_new(
            &self.output,
            &json_bytes(
                &json!({"artifact_id": artifact_id, "artifact_digest": digest, "source_attempt": attempt}),
            )?,
        )?;
        Ok(())
    }
}

struct RunScope<'a> {
    workflow_id: u64,
    repository_id: u64,
    repository: &'a str,
    default_branch: &'a str,
    run_name: &'a str,
}

fn validate_run_identity(
    run: &Value,
    expected_id: u64,
    scope: &RunScope<'_>,
    completed: bool,
) -> Result<()> {
    ensure!(
        positive_number(field(run, "id")?, "workflow run ID")? == expected_id,
        "workflow run ID does not match the requested identity"
    );
    ensure!(
        positive_number(field(run, "workflow_id")?, "workflow ID")? == scope.workflow_id,
        "workflow run belongs to a different workflow"
    );
    ensure!(
        positive_number(field(field(run, "repository")?, "id")?, "repository ID")?
            == scope.repository_id,
        "workflow run belongs to a different repository"
    );
    ensure!(
        string_field(field(run, "repository")?, "full_name")?
            .eq_ignore_ascii_case(scope.repository),
        "workflow run repository name does not match"
    );
    ensure!(
        string_field(run, "event")? == "workflow_dispatch",
        "workflow run was not manually dispatched"
    );
    ensure!(
        string_field(run, "head_branch")? == scope.default_branch,
        "workflow run is not from the current default branch"
    );
    ensure!(
        string_field(run, "display_title")? == scope.run_name,
        "workflow run name does not match"
    );
    ensure!(
        string_field(run, "path")?.split('@').next()
            == Some(".github/workflows/".to_owned() + WORKFLOW_FILE).as_deref(),
        "workflow run path does not match"
    );
    if completed {
        ensure!(
            string_field(run, "conclusion")? == "success",
            "selected workflow run did not succeed"
        );
    }
    Ok(())
}

#[derive(Debug, Args)]
pub(super) struct PrepareApply {
    #[arg(long)]
    pub(super) current_event: PathBuf,
    #[arg(long)]
    pub(super) source_package: PathBuf,
    #[arg(long)]
    pub(super) package_root: PathBuf,
    #[arg(long)]
    pub(super) target: PathBuf,
    #[arg(long)]
    pub(super) run_temp: PathBuf,
    #[arg(long)]
    pub(super) repository: String,
    #[arg(long)]
    pub(super) run_name: String,
    #[arg(long)]
    pub(super) workflow_sha: String,
    #[arg(long)]
    pub(super) current_run_id: String,
    #[arg(long)]
    pub(super) current_attempt: String,
    #[arg(long)]
    pub(super) plan_run_id: String,
    #[arg(long)]
    pub(super) source_attempt: String,
    #[arg(long)]
    pub(super) approve_plan_sha: String,
    #[arg(long)]
    pub(super) recovery_key: String,
    #[arg(long)]
    pub(super) github_output: PathBuf,
}

impl PrepareApply {
    pub(super) fn run(self) -> Result<()> {
        let plan_run_id = parse_canonical_positive(&self.plan_run_id, "plan_run_id")?;
        let source_attempt = parse_canonical_positive(&self.source_attempt, "source run attempt")?;
        let current_run_id =
            parse_canonical_positive(&self.current_run_id, "current workflow run ID")?;
        let current_attempt =
            parse_canonical_positive(&self.current_attempt, "current workflow run attempt")?;
        ensure_sha(&self.approve_plan_sha, "approve_plan_sha")?;
        ensure_workflow_sha(&self.workflow_sha)?;
        let target_bytes = canonical_target(&self.approve_plan_sha)?;
        let target = fs::read(&self.target).context("read exact plan-set target")?;
        ensure!(
            target == target_bytes,
            "plan-set target bytes do not match the approved plan identity"
        );
        let expected_key = format!("plan-set-{}", sha256(&target_bytes));
        ensure!(
            self.recovery_key == expected_key,
            "recovery key does not match the canonical plan-set target"
        );

        let current_bytes =
            fs::read(&self.current_event).context("read current apply dispatch event")?;
        let current_event: Value = parse_json(&current_bytes, "current apply dispatch event")?;
        validate_dispatch_inputs(
            &current_event,
            "apply",
            Some(&self.plan_run_id),
            Some(&self.approve_plan_sha),
        )?;

        let source_event_path = package_file(&self.source_package, "events/trigger-event.json")?;
        let source_event_bytes =
            fs::read(&source_event_path).context("read source prepare trigger event")?;
        let source_event: Value = parse_json(&source_event_bytes, "source prepare trigger event")?;
        validate_dispatch_inputs(&source_event, "prepare", None, None)?;
        let source_event_sha = sha256(&source_event_bytes);
        let source_context = read_package_json(&self.source_package, "run-context.json")?;
        ensure!(
            string_field(&source_context, "workflow_file")? == WORKFLOW_FILE,
            "source run context workflow does not match"
        );
        ensure!(
            string_field(&source_context, "repository")?.eq_ignore_ascii_case(&self.repository),
            "source run context repository does not match"
        );
        ensure!(
            string_field(&source_context, "recovery_key")? == "workflow-history-v2",
            "source run context recovery key is invalid"
        );
        ensure!(
            string_field(&source_context, "run_name")? == self.run_name,
            "source run context name is invalid"
        );
        let source_workflow_sha = string_field(&source_context, "trusted_source_sha")?;
        ensure_workflow_sha(&source_workflow_sha)?;
        ensure!(
            positive_number(
                field(&source_context, "workflow_run_id")?,
                "source context run ID"
            )? == plan_run_id,
            "source run context ID does not match the selected run"
        );
        ensure!(
            positive_number(
                field(&source_context, "workflow_run_attempt")?,
                "source context attempt"
            )? == source_attempt,
            "source run context attempt does not match the selected run"
        );
        ensure!(
            string_field(&source_context, "phase")? == "completed",
            "source preview is not a completed native no-op context"
        );
        ensure!(
            source_context
                .get("dispatch_steps")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty),
            "source no-op context contains mutation steps"
        );
        let source_context_plans = source_context
            .get("plans")
            .and_then(Value::as_array)
            .context("source native no-op context has no plan list")?;
        ensure!(
            source_context_plans.len() == 1,
            "source native no-op context must contain exactly one no-op plan"
        );
        ensure!(
            string_field(&source_context_plans[0], "name")? == "workflow-noop"
                && string_field(&source_context_plans[0], "command")? == "workflow-noop"
                && string_field(&source_context_plans[0], "path")? == "plans/workflow-noop.json"
                && string_field(&source_context_plans[0], "status")? == "completed",
            "source native context does not contain the exact completed no-op plan"
        );
        let source_decision: Value =
            read_package_json(&self.source_package, "decisions/workflow-noop.json")?;
        ensure!(
            source_decision
                .get("schema_version")
                .and_then(Value::as_u64)
                == Some(1),
            "source preview decision schema is invalid"
        );
        ensure!(
            string_field(&source_decision, "decision")? == "preview-only",
            "source package is not a preview-only no-op"
        );
        ensure!(
            string_field(&source_decision, "repository")?.eq_ignore_ascii_case(&self.repository),
            "source preview decision repository does not match"
        );
        ensure!(
            string_field(&source_decision, "workflow_file")? == WORKFLOW_FILE,
            "source preview decision workflow does not match"
        );
        ensure!(
            string_field(&source_decision, "event_name")? == "workflow_dispatch",
            "source preview decision event is not workflow_dispatch"
        );
        ensure!(
            string_field(&source_decision, "event_sha256")? == source_event_sha,
            "source preview decision does not bind its trigger event"
        );
        ensure!(
            positive_number(field(&source_decision, "run_id")?, "source decision run ID")?
                == plan_run_id,
            "source preview decision run ID does not match"
        );
        ensure!(
            positive_number(
                field(&source_decision, "attempt")?,
                "source decision attempt"
            )? == source_attempt,
            "source preview decision attempt does not match"
        );
        ensure!(
            source_decision.get("attempt_target")
                == Some(&json!({"event_sha256": source_event_sha})),
            "source preview decision target does not bind its trigger event"
        );
        ensure!(
            source_context.get("attempt_target") == source_decision.get("attempt_target"),
            "source preview decision target does not match native context"
        );
        ensure!(
            string_field(&source_decision, "recovery_key")? == "workflow-history-v2",
            "source preview decision recovery key is invalid"
        );
        ensure!(
            string_field(&source_decision, "workflow_sha")? == source_workflow_sha,
            "source preview decision workflow SHA does not match the native context"
        );
        ensure!(
            source_decision.get("proposal").is_none(),
            "source preview decision must not contain a proposal"
        );
        let preview_refs = source_decision
            .get("previews")
            .and_then(Value::as_array)
            .context("source preview references are missing")?;
        ensure!(
            preview_refs.len() == 1,
            "source preview must refer to exactly one execution plan"
        );
        ensure!(
            string_field(&preview_refs[0], "path")? == "previews/execution.json",
            "source preview path is unexpected"
        );
        let preview_path = package_file(&self.source_package, "previews/execution.json")?;
        let preview_bytes =
            fs::read(&preview_path).context("read retained raw execution preview")?;
        let preview_file_sha = sha256(&preview_bytes);
        ensure!(
            string_field(&preview_refs[0], "sha256")? == preview_file_sha,
            "source preview reference does not match raw plan bytes"
        );
        let plan: Value = parse_json(&preview_bytes, "retained execution plan")?;
        ensure!(
            plan.get("schema_version").and_then(Value::as_u64) == Some(2),
            "retained execution plan schema is invalid"
        );
        ensure!(
            string_field(&plan, "command")? == PLAN_COMMAND,
            "retained plan command is invalid"
        );
        let plan_repository = field(&plan, "repository")?;
        let owner = string_field(plan_repository, "owner")?;
        let name = string_field(plan_repository, "name")?;
        ensure!(
            format!("{owner}/{name}").eq_ignore_ascii_case(&self.repository),
            "retained plan repository does not match"
        );
        ensure!(
            string_field(&plan, "sha256")? == self.approve_plan_sha,
            "retained plan semantic SHA does not match the approval"
        );

        let event_sha = sha256(&current_bytes);
        let review = json!({
            "schema_version": 1,
            "workflow_file": WORKFLOW_FILE,
            "repository": self.repository,
            "plan_name": PLAN_NAME,
            "command": PLAN_COMMAND,
            "plan_sha256": self.approve_plan_sha,
            "approval_input": "approve_plan_sha",
            "workflow_event": "workflow_dispatch",
            "workflow_run_id": current_run_id,
            "workflow_run_attempt": current_attempt,
            "reviewed_plan_run_id": plan_run_id,
            "event_path": "events/dispatch-event.json",
            "event_sha256": event_sha,
        });
        ensure!(
            review.as_object().is_some_and(|object| object.len() == 13),
            "review sidecar field count is invalid"
        );
        let review_bytes = json_bytes(&review)?;
        let review_sha = sha256(&review_bytes);
        let target_value: Value = parse_json(&target_bytes, "canonical plan-set target")?;
        let context = json!({
            "workflow_file": WORKFLOW_FILE,
            "repository": self.repository,
            "recovery_key": self.recovery_key,
            "run_name": self.run_name,
            "run_id": current_run_id,
            "attempt": current_attempt,
            "attempt_target": target_value,
            "dispatch_steps": [MUTATOR_STEP],
            "trusted_source_sha": self.workflow_sha,
        });
        let context_bytes = json_bytes(&context)?;
        let event_output = self.package_root.join("events/dispatch-event.json");
        let review_output = self.package_root.join("reviews/execution.json");
        let context_output = self
            .run_temp
            .join("execution-state-apply-context-start.json");
        ensure_new(&event_output)?;
        ensure_new(&review_output)?;
        ensure_new(&context_output)?;

        create_private_dir(self.package_root.join("events"))?;
        create_private_dir(self.package_root.join("plans"))?;
        create_private_dir(self.package_root.join("reviews"))?;
        create_private_dir(self.package_root.join("apply-results"))?;
        create_private_dir(self.package_root.join("journal"))?;
        write_new(&event_output, &current_bytes)?;
        write_new(&review_output, &review_bytes)?;
        write_new(&context_output, &context_bytes)?;
        append_outputs(&self.github_output, &[("review_sha256", review_sha)])?;
        Ok(())
    }
}
