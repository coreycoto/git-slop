use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use git_slop_xtask::{
    codex, distribution, finish_validation, homebrew, issue_forms, manifest, release, repository,
    workflows,
};

#[derive(Debug, Parser)]
#[command(
    name = "cargo xtask",
    about = "Private maintainer automation for git-slop"
)]
struct Cli {
    /// Repository root containing the public git-slop Cargo.toml.
    #[arg(long, global = true, default_value = ".")]
    repo_root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate every repository-owned maintainer contract.
    Validate {
        /// Fail when the Codex CLI is unavailable instead of skipping execpolicy checks.
        #[arg(long)]
        require_codex_cli: bool,
    },

    /// Validate Codex configuration, plugins, agents, prompts, and schemas.
    ValidateCodex {
        /// Fail when the Codex CLI is unavailable instead of skipping execpolicy checks.
        #[arg(long)]
        require_codex_cli: bool,
    },

    /// Validate GitHub Actions workflow contracts.
    ValidateWorkflows,

    /// Validate repository issue forms and their contact link.
    CheckIssueForms,

    /// Validate release, package-boundary, and Python-retirement contracts.
    CheckDistribution,

    /// Validate a tagged release and optionally run all local release gates.
    ReleasePrepare {
        #[arg(long)]
        version: String,

        #[arg(long, default_value = "../homebrew-tap")]
        tap: PathBuf,

        /// Validate only the Cargo version and exact tag-to-HEAD identity.
        #[arg(long)]
        check_only: bool,
    },

    /// Generate the deterministic release manifest and SHA256SUMS.
    ReleaseManifest {
        #[arg(long, default_value = "dist")]
        dist_dir: PathBuf,

        #[arg(long, default_value = "dist/release-manifest.json")]
        output: PathBuf,

        #[arg(long, default_value = "dist/SHA256SUMS")]
        checksum_output: PathBuf,

        #[arg(long)]
        tag: Option<String>,
    },

    /// Render the native Rust Homebrew formula from verified release identity.
    HomebrewFormula {
        #[arg(long)]
        manifest: Option<PathBuf>,

        #[arg(long)]
        tag: Option<String>,

        #[arg(long)]
        version: Option<String>,

        #[arg(long)]
        revision: Option<String>,

        #[arg(long, default_value = "../homebrew-tap/Formula/git-slop.rb")]
        formula: PathBuf,
    },
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let repo_root = fs::canonicalize(&cli.repo_root).with_context(|| {
        format!(
            "unable to resolve repository root {}",
            cli.repo_root.display()
        )
    })?;

    match cli.command {
        Command::Validate { require_codex_cli } => {
            let mut errors = codex::validate(&repo_root, require_codex_cli);
            errors.extend(workflows::validate(&repo_root));
            errors.extend(issue_forms::validate(&repo_root));
            errors.extend(repository::validate_overlays(&repo_root));
            errors.extend(distribution::validate(&repo_root));
            finish_validation("Repository", errors)
        }
        Command::ValidateCodex { require_codex_cli } => finish_validation(
            "Codex surface",
            codex::validate(&repo_root, require_codex_cli),
        ),
        Command::ValidateWorkflows => {
            finish_validation("Workflow contracts", workflows::validate(&repo_root))
        }
        Command::CheckIssueForms => {
            finish_validation("Issue forms", issue_forms::validate(&repo_root))
        }
        Command::CheckDistribution => {
            finish_validation("Distribution contracts", distribution::validate(&repo_root))
        }
        Command::ReleasePrepare {
            version,
            tap,
            check_only,
        } => {
            if check_only {
                let state = release::validate_release_state(&repo_root, &version)?;
                println!("Verified local tag {} at {}.", state.tag, state.revision);
                return Ok(());
            }

            let options = release::PrepareReleaseOptions::new(&repo_root, version, tap);
            let prepared = release::prepare_release(&options)?;
            for message in prepared.messages {
                println!("{message}");
            }
            Ok(())
        }
        Command::ReleaseManifest {
            dist_dir,
            output,
            checksum_output,
            tag,
        } => {
            let dist_dir = manifest::resolve_project_path(&repo_root, &dist_dir)?;
            let generated = manifest::build_manifest(&repo_root, &dist_dir, tag.as_deref())?;
            let paths = manifest::write_manifest_outputs(
                &repo_root,
                &dist_dir,
                &generated,
                &output,
                &checksum_output,
            )?;
            println!("Wrote release manifest: {}", paths.manifest.display());
            println!("Wrote checksums: {}", paths.checksums.display());
            Ok(())
        }
        Command::HomebrewFormula {
            manifest,
            tag,
            version,
            revision,
            formula,
        } => {
            let source = homebrew::FormulaSourceArgs {
                manifest,
                tag,
                version,
                revision,
            };
            let identity = homebrew::resolve_formula_source(&repo_root, &source)?;
            let path = homebrew::write_formula(&repo_root, &formula, &identity)?;
            println!("Wrote Homebrew formula: {}", path.display());
            Ok(())
        }
    }
}
