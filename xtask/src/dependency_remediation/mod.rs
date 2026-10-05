mod candidate;
mod paths;
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

    /// Prepare, continue, or verify a trusted publication package.
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
