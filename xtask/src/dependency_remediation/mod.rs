mod artifacts;
mod candidate;
mod noop;
mod paths;
mod proposal;
mod publisher;
mod source;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args as ClapArgs, Subcommand, ValueEnum};

pub use paths::{path_allowed, read_result, validate_result_paths};

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate the proposal's changed-file inventory against repo policy.
    ValidateResult { result: PathBuf },

    /// Capture exact event bytes and bind source/base identities for later jobs.
    CaptureSource,

    /// Test the exact captured source tree without exposing GitHub credentials.
    VerifySource {
        #[arg(long)]
        base_sha: String,
        #[arg(long)]
        source_sha: String,
    },

    /// Verify a run-scoped Actions artifact's exact upstream identity.
    VerifyArtifact {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        digest: String,
    },

    /// Collect a bounded Codex result and its exact immutable source inputs.
    CollectProposal {
        #[arg(long)]
        capture: PathBuf,
        #[arg(long)]
        result: PathBuf,
        #[arg(long)]
        destination: PathBuf,
        #[arg(long)]
        workflow_sha: String,
        #[arg(long)]
        capture_id: String,
        #[arg(long)]
        capture_name: String,
        #[arg(long)]
        capture_digest: String,
    },

    /// Prepare exact no-op inputs before gh-steward records and settles them.
    PrepareNoop {
        #[arg(long)]
        capture: PathBuf,
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        context: PathBuf,
    },

    /// Verify that a proposal reuses the exact credential-free source handoff.
    ValidateCandidateInputs { capture: PathBuf, proposal: PathBuf },

    /// Apply the exact supplemental patch and bind its staged tree to the proposal.
    ApplyCandidate {
        proposal: PathBuf,
        source_sha: String,
        tree_file: PathBuf,
    },

    /// Verify the credential-free tests left the exact source and candidate intact.
    VerifyCandidate {
        source_sha: String,
        tree_file: PathBuf,
    },

    /// Bind the verified tree and exact source/proposal artifacts into a candidate handoff.
    CreateCandidateEvidence {
        capture: PathBuf,
        proposal: PathBuf,
        destination: PathBuf,
    },

    /// Request publication (deferred until gh-steward owns GitHub mutation authority).
    Publish {
        #[arg(value_enum)]
        action: PublishAction,
        package: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PublishAction {
    Prepare,
    Push,
    CreatePr,
    VerifyPr,
    Continue,
}

impl Args {
    pub fn run(self, repo_root: &std::path::Path) -> Result<()> {
        match self.command {
            Command::ValidateResult { result } => {
                let result = paths::read_result(&result)?;
                paths::validate_result_paths(&result)?;
                println!("Dependency-remediation result paths passed.");
                Ok(())
            }
            Command::CaptureSource => source::capture(),
            Command::VerifySource {
                base_sha,
                source_sha,
            } => source::verify_source(repo_root, &base_sha, &source_sha),
            Command::VerifyArtifact { id, name, digest } => artifacts::verify(&id, &name, &digest),
            Command::CollectProposal {
                capture,
                result,
                destination,
                workflow_sha,
                capture_id,
                capture_name,
                capture_digest,
            } => proposal::collect(
                &capture,
                &result,
                &destination,
                &workflow_sha,
                &capture_id,
                &capture_name,
                &capture_digest,
            ),
            Command::PrepareNoop {
                capture,
                package,
                context,
            } => noop::prepare(&capture, &package, &context),
            Command::ValidateCandidateInputs { capture, proposal } => {
                source::validate_candidate_inputs(&capture, &proposal)
            }
            Command::ApplyCandidate {
                proposal,
                source_sha,
                tree_file,
            } => candidate::apply(repo_root, &proposal, &source_sha, &tree_file),
            Command::VerifyCandidate {
                source_sha,
                tree_file,
            } => candidate::verify(repo_root, &source_sha, &tree_file),
            Command::CreateCandidateEvidence {
                capture,
                proposal,
                destination,
            } => candidate::create_evidence(&capture, &proposal, &destination),
            Command::Publish { action, package } => publisher::run(
                repo_root,
                match action {
                    PublishAction::Prepare => "prepare",
                    PublishAction::Push => "push",
                    PublishAction::CreatePr => "create-pr",
                    PublishAction::VerifyPr => "verify-pr",
                    PublishAction::Continue => "continue",
                },
                &package,
            ),
        }
    }
}

#[cfg(test)]
mod tests;
