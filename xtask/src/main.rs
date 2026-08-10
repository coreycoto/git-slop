use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use git_slop_xtask::{
    codex, crates_io, distribution, finish_validation, homebrew, issue_forms, manifest, release,
    repository, sbom, workflows,
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

    /// Generate the public release workflow from independently reviewed stage fragments.
    GenerateReleaseWorkflow {
        /// Verify the generated workflow is current without writing it.
        #[arg(long)]
        check: bool,
    },

    /// Validate repository issue forms and their contact link.
    CheckIssueForms,

    /// Validate release, package-boundary, and removed-runtime contracts.
    CheckDistribution,

    /// Validate a release candidate before its protected workflow creates the tag.
    ReleasePrepare {
        #[arg(long)]
        version: String,

        /// Validate only the Cargo version and candidate HEAD identity.
        #[arg(long)]
        check_only: bool,
    },

    /// Verify a downloaded crates.io package and write canonical source metadata.
    VerifyCrate {
        #[arg(long)]
        crate_file: PathBuf,

        #[arg(long)]
        version: String,

        #[arg(long)]
        revision: String,

        #[arg(long)]
        expected_sha256: String,

        #[arg(long, default_value = "dist/crate-source.json")]
        output: PathBuf,
    },

    /// Generate the deterministic release manifest and SHA256SUMS.
    ReleaseManifest {
        #[arg(long, default_value = "dist")]
        dist_dir: PathBuf,

        #[arg(long, default_value = "dist/release-manifest.json")]
        output: PathBuf,

        #[arg(long, default_value = "dist/SHA256SUMS")]
        checksum_output: PathBuf,

        #[arg(long, default_value = "dist/crate-source.json")]
        crate_source: PathBuf,

        #[arg(long)]
        tag: Option<String>,
    },

    /// Render the native Rust Homebrew formula from verified release identity.
    HomebrewFormula {
        #[arg(long)]
        manifest: PathBuf,

        #[arg(long, default_value = "../homebrew-tap/Formula/git-slop.rb")]
        formula: PathBuf,
    },

    /// Generate deterministic CycloneDX 1.5 and SPDX 2.3 SBOM documents.
    Sbom {
        #[arg(long, default_value = "dist")]
        output_dir: PathBuf,
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
        Command::GenerateReleaseWorkflow { check } => {
            workflows::generate_release_workflow(&repo_root, check)?;
            println!(
                "{} release-publish.yml from stage fragments.",
                if check { "Verified" } else { "Generated" }
            );
            Ok(())
        }
        Command::CheckIssueForms => {
            finish_validation("Issue forms", issue_forms::validate(&repo_root))
        }
        Command::CheckDistribution => {
            finish_validation("Distribution contracts", distribution::validate(&repo_root))
        }
        Command::ReleasePrepare {
            version,
            check_only,
        } => {
            if check_only {
                let state = release::validate_release_state(&repo_root, &version)?;
                println!(
                    "Verified release candidate HEAD {} for future tag {}.",
                    state.revision, state.tag
                );
                return Ok(());
            }

            let options = release::PrepareReleaseOptions::new(&repo_root, version);
            let prepared = release::prepare_release(&options)?;
            for message in prepared.messages {
                println!("{message}");
            }
            Ok(())
        }
        Command::VerifyCrate {
            crate_file,
            version,
            revision,
            expected_sha256,
            output,
        } => {
            let options = crates_io::VerifyCrateOptions {
                project_root: repo_root,
                crate_file,
                version,
                revision,
                expected_sha256,
                output: output.clone(),
            };
            let source = crates_io::verify_crate(&options)?;
            println!("Verified canonical crate: {}", source.url);
            println!("Wrote crate source: {}", output.display());
            Ok(())
        }
        Command::ReleaseManifest {
            dist_dir,
            output,
            checksum_output,
            crate_source,
            tag,
        } => {
            let dist_dir = manifest::resolve_project_path(&repo_root, &dist_dir)?;
            let crate_source = crates_io::load_crate_source(&repo_root, &crate_source)?;
            let generated =
                manifest::build_manifest(&repo_root, &dist_dir, &crate_source, tag.as_deref())?;
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
        Command::HomebrewFormula { manifest, formula } => {
            let identity = homebrew::load_manifest(&repo_root, &manifest)?;
            let path = homebrew::write_formula(&repo_root, &formula, &identity)?;
            println!("Wrote Homebrew formula: {}", path.display());
            Ok(())
        }
        Command::Sbom { output_dir } => {
            for path in sbom::generate(&repo_root, &output_dir)? {
                println!("Wrote SBOM: {}", path.display());
            }
            Ok(())
        }
    }
}
