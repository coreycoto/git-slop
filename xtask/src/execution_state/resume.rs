//! Retained approval and journal checks for a native recovered apply.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::Value;

use super::{
    PLAN_COMMAND, PLAN_NAME, WORKFLOW_FILE, append_outputs, canonical_target, ensure_sha, field,
    parse_canonical_positive, parse_json, positive_number, read_package_file, read_package_json,
    sha256, string_field, validate_dispatch_inputs,
};

#[derive(Debug, Args)]
pub(super) struct CheckResumedApply {
    #[arg(long)]
    pub(super) package_root: PathBuf,
    #[arg(long)]
    pub(super) repository: String,
    #[arg(long)]
    pub(super) plan_run_id: String,
    #[arg(long)]
    pub(super) approve_plan_sha: String,
    #[arg(long)]
    pub(super) recovery_key: String,
    #[arg(long)]
    pub(super) github_output: PathBuf,
}

impl CheckResumedApply {
    pub(super) fn run(self) -> Result<()> {
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
