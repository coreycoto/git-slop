use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod io;
use io::{
    append_outputs, create_private_dir, ensure_new, field, inputs, json_bytes, optional_string,
    package_file, parse_json, positive_number, read_json, read_json_documents, read_package_file,
    read_package_json, string_field, write_new,
};

const WORKFLOW_FILE: &str = "execution_state_sync.yml";
const PLAN_NAME: &str = "execution";
const PLAN_COMMAND: &str = "execution-sync";
const MUTATOR_STEP: &str = "Apply exact reviewed execution plan";
const HANDOFF_SUFFIX: &str = "-handoff-00";

#[derive(Debug, Args)]
pub struct CliArgs {
    #[command(subcommand)]
    command: Command,
}

impl CliArgs {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::PrepareTarget(args) => args.run(),
            Command::ProjectSnapshot(args) => args.run(),
            Command::PrepareSelector(args) => args.run(),
            Command::PrepareNoop(args) => args.run(),
            Command::ValidateApplyInputs(args) => args.run(),
            Command::SelectHandoff(args) => args.run(),
            Command::PrepareApply(args) => args.run(),
            Command::CheckResumedApply(args) => args.run(),
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a prepare dispatch and separate package trigger evidence from diagnostics.
    PrepareTarget(PrepareTarget),
    /// Validate and reduce a native Project snapshot to its exact selector identity.
    ProjectSnapshot(ProjectSnapshot),
    /// Build the native execution selector from a validated target and optional Project scope.
    PrepareSelector(PrepareSelector),
    /// Build the plan-free native no-op context and preview-only decision.
    PrepareNoop(PrepareNoop),
    /// Validate apply selectors and write the exact canonical plan-set target.
    ValidateApplyInputs(ValidateApplyInputs),
    /// Select one immutable handoff artifact from already-downloaded GitHub API data.
    SelectHandoff(SelectHandoff),
    /// Validate a source preview and write this dispatch's exact review envelope/context.
    PrepareApply(PrepareApply),
    /// Compare new apply selectors with a restored approval without changing saved bytes.
    CheckResumedApply(CheckResumedApply),
}

#[derive(Debug, Args)]
struct PrepareTarget {
    #[arg(long)]
    operation: String,
    #[arg(long)]
    pr_number: String,
    #[arg(long)]
    issue_number: String,
    #[arg(long)]
    plan_run_id: String,
    #[arg(long)]
    approve_plan_sha: String,
    #[arg(long)]
    event: PathBuf,
    #[arg(long)]
    initial_root: PathBuf,
    #[arg(long)]
    diagnostics_root: PathBuf,
    #[arg(long)]
    github_output: PathBuf,
}

impl PrepareTarget {
    fn run(self) -> Result<()> {
        ensure!(self.operation == "prepare", "prepare operation is required");
        ensure!(
            self.plan_run_id.is_empty() && self.approve_plan_sha.is_empty(),
            "prepare does not accept apply plan selectors"
        );
        let (kind, value) = match (self.pr_number.is_empty(), self.issue_number.is_empty()) {
            (false, true) => ("pull-request", self.pr_number.as_str()),
            (true, false) => ("issue", self.issue_number.as_str()),
            _ => bail!("prepare requires exactly one of pr_number or issue_number"),
        };
        parse_canonical_positive(value, "prepare target")?;
        let event = read_json(&self.event)?;
        validate_dispatch_inputs(&event, "prepare", None, None)?;
        let event_inputs = inputs(&event)?;
        ensure!(
            string_field(
                event_inputs,
                if kind == "issue" {
                    "issue_number"
                } else {
                    "pr_number"
                }
            )? == value,
            "prepare target does not match the raw workflow_dispatch event"
        );
        ensure!(
            optional_string(
                event_inputs,
                if kind == "issue" {
                    "pr_number"
                } else {
                    "issue_number"
                }
            )?
            .is_empty(),
            "raw workflow_dispatch event contains both prepare target selectors"
        );
        let event_bytes = fs::read(&self.event).context("read raw workflow_dispatch event")?;
        let trigger_path = self.initial_root.join("events/trigger-event.json");
        let diagnostic_event = self.diagnostics_root.join("events/dispatch-event.json");
        ensure_new(&trigger_path)?;
        ensure_new(&diagnostic_event)?;

        create_private_dir(self.initial_root.join("events"))?;
        create_private_dir(self.diagnostics_root.join("events"))?;
        write_new(&trigger_path, &event_bytes)?;
        write_new(&diagnostic_event, &event_bytes)?;
        append_outputs(
            &self.github_output,
            &[
                ("kind", kind.to_owned()),
                ("number", value.to_owned()),
                ("initial_root", self.initial_root.display().to_string()),
            ],
        )?;
        Ok(())
    }
}

#[derive(Debug, Args)]
struct ProjectSnapshot {
    #[arg(long)]
    snapshot: PathBuf,
    #[arg(long)]
    owner: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    number: String,
    #[arg(long)]
    host: String,
    #[arg(long)]
    output: PathBuf,
}

impl ProjectSnapshot {
    fn run(self) -> Result<()> {
        let number = parse_canonical_positive(&self.number, "Project number")?;
        let snapshot = read_json(&self.snapshot)?;
        let project = field(field(&snapshot, "data")?, "project")?;
        ensure!(
            string_field(project, "owner_login")?.eq_ignore_ascii_case(&self.owner),
            "native Project snapshot owner does not match config"
        );
        ensure!(
            string_field(project, "owner_type")? == "User",
            "native Project snapshot owner type is not User"
        );
        ensure!(
            string_field(project, "title")? == self.title,
            "native Project snapshot title does not match config"
        );
        ensure!(
            positive_number(field(project, "number")?, "native Project number")? == number,
            "native Project snapshot number does not match config"
        );
        let id = string_field(project, "id")?;
        ensure!(!id.is_empty(), "native Project snapshot has no stable ID");
        let scope = json!({
            "host": self.host,
            "owner": self.owner,
            "owner_type": "User",
            "number": number,
            "id": id,
            "title": self.title,
        });
        ensure_new(&self.output)?;
        write_new(&self.output, &json_bytes(&scope)?)
    }
}

#[derive(Debug, Args)]
struct PrepareSelector {
    #[arg(long)]
    kind: String,
    #[arg(long)]
    number: String,
    #[arg(long)]
    project_available: String,
    #[arg(long)]
    project_scope: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

impl PrepareSelector {
    fn run(self) -> Result<()> {
        let number = parse_canonical_positive(&self.number, "prepare target")?;
        ensure!(
            matches!(self.kind.as_str(), "issue" | "pull-request"),
            "prepare target kind is invalid"
        );
        let available = match self.project_available.as_str() {
            "true" => true,
            "false" => false,
            _ => bail!("Project availability must be true or false"),
        };
        let project = if available {
            let scope = read_json(&self.project_scope)?;
            ensure!(
                scope.is_object(),
                "validated Project selector is not an object"
            );
            Some(scope)
        } else {
            None
        };
        let selector = json!({
            "issue_number": if self.kind == "issue" { number } else { 0 },
            "pull_request_number": if self.kind == "pull-request" { number } else { 0 },
            "skip_project_sync": !available,
            "project": project,
        });
        ensure_new(&self.output)?;
        write_new(&self.output, &json_bytes(&selector)?)
    }
}

#[derive(Debug, Args)]
struct PrepareNoop {
    #[arg(long)]
    package_root: PathBuf,
    #[arg(long)]
    run_temp: PathBuf,
    #[arg(long)]
    repository: String,
    #[arg(long)]
    server_url: String,
    #[arg(long)]
    run_name: String,
    #[arg(long)]
    workflow_sha: String,
    #[arg(long)]
    event_name: String,
    #[arg(long)]
    run_id: String,
    #[arg(long)]
    attempt: String,
    #[arg(long)]
    github_output: PathBuf,
}

impl PrepareNoop {
    fn run(self) -> Result<()> {
        ensure!(
            self.event_name == "workflow_dispatch",
            "prepare no-op requires workflow_dispatch"
        );
        let run_id = parse_canonical_positive(&self.run_id, "workflow run ID")?;
        let attempt = parse_canonical_positive(&self.attempt, "workflow run attempt")?;
        ensure_workflow_sha(&self.workflow_sha)?;
        let root = &self.package_root;
        let event_path = package_file(root, "events/trigger-event.json")?;
        let event_bytes = fs::read(&event_path).context("read retained trigger event")?;
        let event: Value = parse_json(&event_bytes, "retained trigger event")?;
        validate_dispatch_inputs(&event, "prepare", None, None)?;
        let preview_path = package_file(root, "previews/execution.json")?;
        let preview_bytes = fs::read(preview_path).context("read raw execution preview")?;
        let preview_sha = sha256(&preview_bytes);
        let plan: Value = parse_json(&preview_bytes, "execution preview plan")?;
        ensure!(
            plan.get("schema_version").and_then(Value::as_u64) == Some(2),
            "execution preview plan schema is invalid"
        );
        ensure!(
            string_field(&plan, "command")? == PLAN_COMMAND,
            "execution preview plan command is invalid"
        );
        let plan_repository = field(&plan, "repository")?;
        ensure!(
            format!(
                "{}/{}",
                string_field(plan_repository, "owner")?,
                string_field(plan_repository, "name")?
            )
            .eq_ignore_ascii_case(&self.repository),
            "execution preview plan repository does not match"
        );
        let plan_sha = string_field(&plan, "sha256")?;
        ensure_sha(&plan_sha, "execution preview plan semantic SHA")?;
        let event_sha = sha256(&event_bytes);
        let target = json!({"event_sha256": event_sha});
        let context = json!({
            "workflow_file": WORKFLOW_FILE,
            "repository": self.repository,
            "recovery_key": "workflow-history-v2",
            "run_name": self.run_name,
            "run_id": run_id,
            "attempt": attempt,
            "attempt_target": target,
            "dispatch_steps": [],
            "trusted_source_sha": self.workflow_sha,
        });
        let decision = json!({
            "schema_version": 1,
            "decision": "preview-only",
            "repository": self.repository,
            "server_url": self.server_url,
            "workflow_file": WORKFLOW_FILE,
            "run_id": run_id,
            "attempt": attempt,
            "recovery_key": "workflow-history-v2",
            "attempt_target": target,
            "workflow_sha": self.workflow_sha,
            "event_name": self.event_name,
            "event_sha256": event_sha,
            "previews": [{"path": "previews/execution.json", "sha256": preview_sha}],
        });
        let context_path = self.run_temp.join("execution-state-noop-context.json");
        let decision_path = self.run_temp.join("execution-state-noop-decision.json");
        ensure_new(&context_path)?;
        ensure_new(&decision_path)?;
        write_new(&context_path, &json_bytes(&context)?)?;
        write_new(&decision_path, &json_bytes(&decision)?)?;
        append_outputs(
            &self.github_output,
            &[
                ("plan_sha", plan_sha),
                ("preview_sha", preview_sha),
                ("ready", "true".to_owned()),
            ],
        )?;
        Ok(())
    }
}

#[derive(Debug, Args)]
struct ValidateApplyInputs {
    #[arg(long)]
    operation: String,
    #[arg(long)]
    pr_number: String,
    #[arg(long)]
    issue_number: String,
    #[arg(long)]
    plan_run_id: String,
    #[arg(long)]
    approve_plan_sha: String,
    #[arg(long)]
    event: PathBuf,
    #[arg(long)]
    target: PathBuf,
    #[arg(long)]
    github_output: PathBuf,
}

impl ValidateApplyInputs {
    fn run(self) -> Result<()> {
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PlanTarget {
    plans: [PlanIdentity; 1],
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PlanIdentity {
    command: String,
    name: String,
    sha256: String,
}

fn canonical_target(plan_sha: &str) -> Result<Vec<u8>> {
    ensure_sha(plan_sha, "approved plan SHA-256")?;
    let target = PlanTarget {
        plans: [PlanIdentity {
            command: PLAN_COMMAND.to_owned(),
            name: PLAN_NAME.to_owned(),
            sha256: plan_sha.to_owned(),
        }],
    };
    serde_json::to_vec(&target).context("serialize canonical plan-set target")
}

fn validate_apply(
    operation: String,
    pr_number: &str,
    issue_number: &str,
    plan_run_id: &str,
    approve_plan_sha: &str,
    event_path: &Path,
) -> Result<String> {
    ensure!(operation == "apply", "apply operation is required");
    ensure!(
        pr_number.is_empty() && issue_number.is_empty(),
        "apply does not accept prepare target selectors"
    );
    parse_canonical_positive(plan_run_id, "plan_run_id")?;
    ensure_sha(approve_plan_sha, "approve_plan_sha")?;
    let event = read_json(event_path)?;
    validate_dispatch_inputs(&event, "apply", Some(plan_run_id), Some(approve_plan_sha))?;
    let bytes = canonical_target(approve_plan_sha)?;
    Ok(format!("plan-set-{}", sha256(&bytes)))
}

#[derive(Debug, Args)]
struct SelectHandoff {
    #[arg(long)]
    current_run: PathBuf,
    #[arg(long)]
    selected_run: PathBuf,
    #[arg(long)]
    artifact_pages: PathBuf,
    #[arg(long)]
    repository: String,
    #[arg(long)]
    current_run_id: String,
    #[arg(long)]
    default_branch: String,
    #[arg(long)]
    run_name: String,
    #[arg(long)]
    plan_run_id: String,
    #[arg(long)]
    output: PathBuf,
}

impl SelectHandoff {
    fn run(self) -> Result<()> {
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
struct PrepareApply {
    #[arg(long)]
    current_event: PathBuf,
    #[arg(long)]
    source_package: PathBuf,
    #[arg(long)]
    package_root: PathBuf,
    #[arg(long)]
    target: PathBuf,
    #[arg(long)]
    run_temp: PathBuf,
    #[arg(long)]
    repository: String,
    #[arg(long)]
    run_name: String,
    #[arg(long)]
    workflow_sha: String,
    #[arg(long)]
    current_run_id: String,
    #[arg(long)]
    current_attempt: String,
    #[arg(long)]
    plan_run_id: String,
    #[arg(long)]
    source_attempt: String,
    #[arg(long)]
    approve_plan_sha: String,
    #[arg(long)]
    recovery_key: String,
    #[arg(long)]
    github_output: PathBuf,
}

impl PrepareApply {
    fn run(self) -> Result<()> {
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

#[derive(Debug, Args)]
struct CheckResumedApply {
    #[arg(long)]
    package_root: PathBuf,
    #[arg(long)]
    repository: String,
    #[arg(long)]
    plan_run_id: String,
    #[arg(long)]
    approve_plan_sha: String,
    #[arg(long)]
    recovery_key: String,
    #[arg(long)]
    github_output: PathBuf,
}

impl CheckResumedApply {
    fn run(self) -> Result<()> {
        parse_canonical_positive(&self.plan_run_id, "plan_run_id")?;
        ensure_sha(&self.approve_plan_sha, "approve_plan_sha")?;
        let recovery_source = read_package_json(&self.package_root, "recovery-source.json")?;
        ensure!(
            recovery_source.is_object(),
            "native recovery source proof is not an object"
        );
        let context = read_package_json(&self.package_root, "run-context.json")?;
        let plans = context
            .get("plans")
            .and_then(Value::as_array)
            .context("restored plan entries are missing")?;
        ensure!(
            plans.len() == 1,
            "restored execution context does not contain exactly one plan"
        );
        let entry = &plans[0];
        ensure!(
            string_field(entry, "name")? == PLAN_NAME
                && string_field(entry, "command")? == PLAN_COMMAND,
            "restored plan identity is invalid"
        );
        ensure!(
            string_field(entry, "path")? == "plans/execution.json",
            "restored plan path is invalid"
        );
        ensure!(
            string_field(entry, "review_path")? == "reviews/execution.json",
            "restored review path is invalid"
        );
        ensure!(
            string_field(&context, "workflow_file")? == WORKFLOW_FILE,
            "restored workflow identity is invalid"
        );
        ensure!(
            string_field(&context, "repository")?.eq_ignore_ascii_case(&self.repository),
            "restored repository identity is invalid"
        );

        let expected_target_bytes = canonical_target(&self.approve_plan_sha)?;
        let expected_target: Value =
            parse_json(&expected_target_bytes, "expected plan-set target")?;
        let context_target_matches = context.get("recovery_key").and_then(Value::as_str)
            == Some(self.recovery_key.as_str())
            && context.get("attempt_target") == Some(&expected_target)
            && self.recovery_key == format!("plan-set-{}", sha256(&expected_target_bytes));
        let plan_bytes = read_package_file(&self.package_root, "plans/execution.json")?;
        let review_bytes = read_package_file(&self.package_root, "reviews/execution.json")?;
        let plan: Value = parse_json(&plan_bytes, "restored execution plan")?;
        let review: Value = parse_json(&review_bytes, "restored reviewed-dispatch sidecar")?;
        ensure!(
            plan.get("schema_version").and_then(Value::as_u64) == Some(2),
            "restored execution plan schema is invalid"
        );
        ensure!(
            string_field(&plan, "command")? == PLAN_COMMAND,
            "restored execution plan command is invalid"
        );
        ensure!(
            string_field(&plan, "sha256")? == string_field(entry, "sha256")?,
            "restored plan SHA differs from its native context entry"
        );
        ensure!(
            string_field(&plan, "sha256")? == self.approve_plan_sha || !context_target_matches,
            "restored plan SHA does not match approval"
        );
        let plan_repo = field(&plan, "repository")?;
        ensure!(
            format!(
                "{}/{}",
                string_field(plan_repo, "owner")?,
                string_field(plan_repo, "name")?
            )
            .eq_ignore_ascii_case(&self.repository),
            "restored plan repository is invalid"
        );
        ensure!(
            review.as_object().is_some_and(|object| object.len() == 13),
            "restored review sidecar must have exactly 13 fields"
        );
        let expected_review_keys = [
            "approval_input",
            "command",
            "event_path",
            "event_sha256",
            "plan_name",
            "plan_sha256",
            "repository",
            "reviewed_plan_run_id",
            "schema_version",
            "workflow_event",
            "workflow_file",
            "workflow_run_attempt",
            "workflow_run_id",
        ];
        ensure!(
            review
                .as_object()
                .is_some_and(|object| object.keys().map(String::as_str).eq(expected_review_keys)),
            "restored review sidecar fields do not match the reviewed-dispatch contract"
        );
        ensure!(
            string_field(&review, "workflow_file")? == WORKFLOW_FILE,
            "restored review workflow is invalid"
        );
        ensure!(
            string_field(&review, "repository")?.eq_ignore_ascii_case(&self.repository),
            "restored review repository is invalid"
        );
        ensure!(
            string_field(&review, "plan_name")? == PLAN_NAME,
            "restored review plan name is invalid"
        );
        ensure!(
            string_field(&review, "command")? == PLAN_COMMAND,
            "restored review command is invalid"
        );
        ensure!(
            string_field(&review, "approval_input")? == "approve_plan_sha",
            "restored review approval input is invalid"
        );
        ensure!(
            string_field(&review, "workflow_event")? == "workflow_dispatch",
            "restored review event is invalid"
        );
        ensure!(
            string_field(&review, "event_path")? == "events/dispatch-event.json",
            "restored review event path is invalid"
        );
        positive_number(field(&review, "workflow_run_id")?, "review workflow run ID")?;
        positive_number(
            field(&review, "workflow_run_attempt")?,
            "review workflow run attempt",
        )?;
        let original_run = review
            .get("reviewed_plan_run_id")
            .and_then(Value::as_u64)
            .context("restored reviewed run ID is invalid")?;
        let review_sha_matches =
            string_field(&review, "plan_sha256")? == string_field(&plan, "sha256")?;
        ensure!(
            review.get("schema_version").and_then(Value::as_u64) == Some(1),
            "restored review sidecar schema is invalid"
        );
        ensure!(
            sha256(&review_bytes) == string_field(entry, "review_sha256")?,
            "restored review bytes differ from the native context digest"
        );
        let event_bytes = read_package_file(&self.package_root, "events/dispatch-event.json")?;
        ensure!(
            sha256(&event_bytes) == string_field(&review, "event_sha256")?,
            "restored review does not bind its exact dispatch event"
        );
        let original_event: Value = parse_json(&event_bytes, "restored apply dispatch event")?;
        validate_dispatch_inputs(
            &original_event,
            "apply",
            Some(&original_run.to_string()),
            Some(string_field(&review, "plan_sha256")?.as_str()),
        )?;
        let selector_matches = original_run.to_string() == self.plan_run_id
            && string_field(&review, "plan_sha256")? == self.approve_plan_sha
            && review_sha_matches;
        let authorization_matches = selector_matches
            && context_target_matches
            && string_field(&plan, "sha256")? == self.approve_plan_sha;

        let phase = string_field(&context, "phase")?;
        let status = string_field(entry, "status")?;
        let journal_id = string_field(entry, "journal_id")?;
        ensure_sha(&journal_id, "restored journal identity")?;
        let journal_path = format!("journal/{journal_id}.json");
        let journal_path_exists = entry.get("journal_path").is_some();
        let apply_result_exists = entry.get("apply_result_path").is_some();
        let journal_file = self.package_root.join(&journal_path);
        let journal_action = if phase == "prepared"
            && status == "prepared"
            && !journal_path_exists
            && !apply_result_exists
        {
            ensure!(
                fs::symlink_metadata(&journal_file)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "prepared restored state unexpectedly contains journal bytes"
            );
            "skip"
        } else {
            ensure!(
                matches!(phase.as_str(), "dispatching" | "unknown")
                    && matches!(status.as_str(), "dispatching" | "unknown"),
                "restored phase/status are not a prepared plan or dispatching/unknown recovery"
            );
            ensure!(
                entry
                    .get("journal_path")
                    .is_none_or(|path| path.as_str() == Some(journal_path.as_str())),
                "restored execution names a conflicting journal path"
            );
            let journal_bytes = read_package_file(&self.package_root, &journal_path)?;
            let journal: Value = parse_json(&journal_bytes, "restored execution journal")?;
            ensure!(
                journal.is_object(),
                "restored execution journal is not an object"
            );
            "install"
        };

        append_outputs(
            &self.github_output,
            &[
                ("ready", "true".to_owned()),
                ("authorization_matches", authorization_matches.to_string()),
                ("journal_action", journal_action.to_owned()),
            ],
        )?;
        if !authorization_matches {
            eprintln!(
                "new operator selectors do not match the retained plan and review; saved package bytes remain unchanged"
            );
        }
        Ok(())
    }
}

fn validate_dispatch_inputs(
    event: &Value,
    operation: &str,
    plan_run_id: Option<&str>,
    plan_sha: Option<&str>,
) -> Result<()> {
    let event_inputs = inputs(event)?;
    ensure!(
        string_field(event_inputs, "operation")? == operation,
        "raw workflow_dispatch operation does not match"
    );
    match operation {
        "prepare" => {
            ensure!(
                optional_string(event_inputs, "plan_run_id")?.is_empty(),
                "prepare event contains an apply run selector"
            );
            ensure!(
                optional_string(event_inputs, "approve_plan_sha")?.is_empty(),
                "prepare event contains an apply SHA selector"
            );
        }
        "apply" => {
            ensure!(
                optional_string(event_inputs, "pr_number")?.is_empty(),
                "apply event contains a prepare PR selector"
            );
            ensure!(
                optional_string(event_inputs, "issue_number")?.is_empty(),
                "apply event contains a prepare issue selector"
            );
            let expected_run = plan_run_id.context("apply validation requires plan_run_id")?;
            let expected_sha = plan_sha.context("apply validation requires approve_plan_sha")?;
            ensure!(
                string_field(event_inputs, "plan_run_id")? == expected_run,
                "raw apply event plan_run_id does not match"
            );
            ensure!(
                string_field(event_inputs, "approve_plan_sha")? == expected_sha,
                "raw apply event approve_plan_sha does not match"
            );
        }
        _ => bail!("unsupported execution-state operation"),
    }
    Ok(())
}

fn parse_canonical_positive(value: &str, label: &str) -> Result<u64> {
    ensure!(
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()),
        "{label} must be a canonical positive integer"
    );
    ensure!(
        value == value.trim_start_matches('0'),
        "{label} must not contain leading zeros"
    );
    let number = value
        .parse::<u64>()
        .with_context(|| format!("{label} exceeds the supported integer range"))?;
    ensure!(
        number > 0 && number <= i64::MAX as u64,
        "{label} is outside the supported positive integer range"
    );
    Ok(number)
}

fn ensure_sha(value: &str, label: &str) -> Result<()> {
    ensure!(
        is_lower_hex(value, 64),
        "{label} must be a lowercase 64-character SHA-256"
    );
    Ok(())
}

fn ensure_workflow_sha(value: &str) -> Result<()> {
    ensure!(
        is_lower_hex(value, 40),
        "trusted workflow SHA must be a lowercase 40-character Git commit SHA"
    );
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
