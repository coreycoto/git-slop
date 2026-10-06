//! Read-only preparation, target selection and preview-only context.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use clap::Args;
use serde_json::{Value, json};

use super::{
    PLAN_COMMAND, WORKFLOW_FILE, append_outputs, create_private_dir, ensure_new, ensure_sha,
    ensure_workflow_sha, field, inputs, json_bytes, optional_string, package_file,
    parse_canonical_positive, parse_json, positive_number, read_json, sha256, string_field,
    validate_dispatch_inputs, write_new,
};

#[derive(Debug, Args)]
pub(super) struct PrepareTarget {
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
    pub(super) initial_root: PathBuf,
    #[arg(long)]
    pub(super) diagnostics_root: PathBuf,
    #[arg(long)]
    pub(super) github_output: PathBuf,
}

impl PrepareTarget {
    pub(super) fn run(self) -> Result<()> {
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
pub(super) struct ProjectSnapshot {
    #[arg(long)]
    pub(super) snapshot: PathBuf,
    #[arg(long)]
    pub(super) owner: String,
    #[arg(long)]
    pub(super) title: String,
    #[arg(long)]
    pub(super) number: String,
    #[arg(long)]
    pub(super) host: String,
    #[arg(long)]
    pub(super) output: PathBuf,
}

impl ProjectSnapshot {
    pub(super) fn run(self) -> Result<()> {
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
pub(super) struct PrepareSelector {
    #[arg(long)]
    pub(super) kind: String,
    #[arg(long)]
    pub(super) number: String,
    #[arg(long)]
    pub(super) project_available: String,
    #[arg(long)]
    pub(super) project_scope: PathBuf,
    #[arg(long)]
    pub(super) output: PathBuf,
}

impl PrepareSelector {
    pub(super) fn run(self) -> Result<()> {
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
pub(super) struct PrepareNoop {
    #[arg(long)]
    pub(super) package_root: PathBuf,
    #[arg(long)]
    pub(super) run_temp: PathBuf,
    #[arg(long)]
    pub(super) repository: String,
    #[arg(long)]
    pub(super) server_url: String,
    #[arg(long)]
    pub(super) run_name: String,
    #[arg(long)]
    pub(super) workflow_sha: String,
    #[arg(long)]
    pub(super) event_name: String,
    #[arg(long)]
    pub(super) run_id: String,
    #[arg(long)]
    pub(super) attempt: String,
    #[arg(long)]
    pub(super) github_output: PathBuf,
}

impl PrepareNoop {
    pub(super) fn run(self) -> Result<()> {
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
