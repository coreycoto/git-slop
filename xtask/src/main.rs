use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use git_slop_xtask::{
    advisor_benchmark, codex, crates_io, developer, distribution, finish_validation, homebrew,
    issue_forms, manifest, release, release_status, repository, sbom, workflows,
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
#[allow(
    clippy::large_enum_variant,
    reason = "the private Clap maintainer CLI keeps benchmark inputs explicit and is parsed once"
)]
enum Command {
    /// Check local contributor prerequisites and print actionable installation guidance.
    Doctor {
        /// Select human progress or one machine-readable terminal receipt.
        #[arg(long, value_enum, default_value_t = ReceiptFormat::Text)]
        format: ReceiptFormat,
    },

    /// Run the complete cross-platform local validation matrix.
    Ci {
        /// Suppress successful gate subprocess output while retaining failures.
        #[arg(long)]
        quiet: bool,
        /// Select human progress or one machine-readable terminal receipt.
        #[arg(long, value_enum, default_value_t = CiFormat::Text)]
        format: CiFormat,
    },

    /// Classify changed files with tested rules and run only the required validation gates.
    VerifyChanged {
        /// Explicit comparison base. Defaults to upstream, origin/main, or HEAD^.
        #[arg(long)]
        base: Option<String>,
        /// Print selected and skipped gates without running them.
        #[arg(long)]
        dry_run: bool,
        /// Select human progress or one machine-readable terminal receipt.
        #[arg(long, value_enum, default_value_t = ReceiptFormat::Text)]
        format: ReceiptFormat,
    },

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

    /// Inspect draft readiness, public immutability, and downstream receiver state.
    ReleaseStatus {
        #[arg(long)]
        version: String,
        /// Select human output or one machine-readable receipt.
        #[arg(long, value_enum, default_value_t = ReceiptFormat::Text)]
        format: ReceiptFormat,
    },

    /// Run the privacy-safe local Safeguard quality and performance matrix.
    AdvisorBenchmark {
        /// Release-mode git-slop binary to benchmark.
        #[arg(long, default_value = "target/release/git-slop")]
        binary: PathBuf,
        /// Reviewed corpus file.
        #[arg(long, default_value = "benchmarks/advisor/corpus-v1.json")]
        corpus: PathBuf,
        /// Preregistered decision thresholds.
        #[arg(long, default_value = "benchmarks/advisor/thresholds-v1.json")]
        thresholds: PathBuf,
        /// Repository mapping in KEY=PATH form; repeat for every corpus repository.
        #[arg(long = "repository", required = true)]
        repositories: Vec<String>,
        /// Local provider adapter used for the benchmark.
        #[arg(long, value_parser = ["ollama", "openai-compatible"], default_value = "ollama")]
        provider: String,
        /// Loopback provider endpoint; defaults to the selected provider's local endpoint.
        #[arg(long)]
        endpoint: Option<String>,
        /// Runtime-specific served model name.
        #[arg(long, default_value = "gpt-oss-safeguard:20b")]
        runtime_model: String,
        /// Exact local runtime name and version.
        #[arg(long)]
        runtime_label: String,
        /// Exact model digest or immutable revision.
        #[arg(long)]
        model_digest: String,
        /// Exact model quantization reported by the local runtime.
        #[arg(long, default_value = "not-applicable")]
        model_quantization: String,
        /// Ignored output directory for aggregate results and the decision report.
        #[arg(long, default_value = "benchmark-results/advisor")]
        output_dir: PathBuf,
        /// Repetitions per matrix cell.
        #[arg(long, default_value_t = 3)]
        repetitions: usize,
        /// Run low/medium/high reasoning across 2K/4K/8K token targets.
        #[arg(long)]
        full_matrix: bool,
        /// Generate fresh deterministic reports and privacy-safe fingerprints without inference.
        #[arg(long)]
        prepare_only: bool,
        /// Optional JSON map of anonymous case IDs to maintainer usefulness ratings from 1 through 5.
        #[arg(long)]
        ratings: Option<PathBuf>,
        /// Explicit private directory outside the repository for one review artifact per case and effort.
        #[arg(long)]
        review_output_dir: Option<PathBuf>,
        /// Stop this Ollama model before the first sample to measure a real cold load.
        #[arg(long)]
        ollama_cold_model: Option<String>,
    },

    /// Apply reviewed manual ratings to a completed advisor benchmark without rerunning inference.
    AdvisorBenchmarkFinalize {
        /// Reviewed corpus file used by the completed benchmark.
        #[arg(long, default_value = "benchmarks/advisor/corpus-v1.json")]
        corpus: PathBuf,
        /// Preregistered thresholds used by the completed benchmark.
        #[arg(long, default_value = "benchmarks/advisor/thresholds-v1.json")]
        thresholds: PathBuf,
        /// Completed machine-readable benchmark results.
        #[arg(long, default_value = "benchmark-results/advisor/results.json")]
        results: PathBuf,
        /// Completed private maintainer ratings covering every anonymous corpus case.
        #[arg(long)]
        ratings: PathBuf,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CiFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ReceiptFormat {
    Text,
    Json,
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
        Command::Doctor { format } => developer::doctor(&repo_root, format == ReceiptFormat::Json),
        Command::Ci { quiet, format } => developer::ci(
            &repo_root,
            quiet || format == CiFormat::Json,
            format == CiFormat::Json,
        ),
        Command::VerifyChanged {
            base,
            dry_run,
            format,
        } => developer::verify_changed(
            &repo_root,
            base.as_deref(),
            dry_run,
            format == ReceiptFormat::Json,
        ),
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
        Command::ReleaseStatus { version, format } => {
            release_status::inspect(&repo_root, &version, format == ReceiptFormat::Json)
        }
        Command::AdvisorBenchmark {
            binary,
            corpus,
            thresholds,
            repositories,
            provider,
            endpoint,
            runtime_model,
            runtime_label,
            model_digest,
            model_quantization,
            output_dir,
            repetitions,
            full_matrix,
            prepare_only,
            ratings,
            review_output_dir,
            ollama_cold_model,
        } => {
            let (results, decision) = advisor_benchmark::run(&advisor_benchmark::Options {
                repo_root,
                binary,
                corpus,
                thresholds,
                repositories,
                endpoint: endpoint.unwrap_or_else(|| {
                    if provider == "ollama" {
                        "http://127.0.0.1:11434/api/chat".to_string()
                    } else {
                        "http://127.0.0.1:11434/v1/chat/completions".to_string()
                    }
                }),
                provider,
                runtime_model,
                runtime_label,
                model_digest,
                model_quantization,
                output_dir,
                repetitions,
                full_matrix,
                prepare_only,
                ratings,
                review_output_dir,
                ollama_cold_model,
            })?;
            println!("Wrote advisor benchmark results: {}", results.display());
            println!("Wrote advisor benchmark decision: {}", decision.display());
            Ok(())
        }
        Command::AdvisorBenchmarkFinalize {
            corpus,
            thresholds,
            results,
            ratings,
        } => {
            let decision =
                advisor_benchmark::finalize(&repo_root, &corpus, &thresholds, &results, &ratings)?;
            println!(
                "Finalized advisor benchmark decision: {}",
                decision.display()
            );
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
