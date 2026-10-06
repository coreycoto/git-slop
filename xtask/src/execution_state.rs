use std::path::Path;

#[cfg(test)]
use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use serde_json::json;
use sha2::{Digest, Sha256};

mod io;
mod prepare;
mod resume;
mod source_apply;

use io::{
    append_outputs, create_private_dir, ensure_new, field, inputs, json_bytes, optional_string,
    package_file, parse_json, positive_number, read_json, read_json_documents, read_package_file,
    read_package_json, string_field, write_new,
};
use prepare::{PrepareNoop, PrepareSelector, PrepareTarget, ProjectSnapshot};
use resume::CheckResumedApply;
use source_apply::{PrepareApply, SelectHandoff, ValidateApplyInputs};

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
