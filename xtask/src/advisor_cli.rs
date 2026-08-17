use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use clap::ValueEnum;
use git_slop_xtask::advisor_benchmark;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CapacityFormat {
    Text,
    Json,
}

#[derive(Debug, Args)]
pub struct CapacityArgs {
    /// Explicit canonical model identity to evaluate without contacting a provider.
    #[arg(long)]
    model: String,
    /// Exact model artifact size in bytes.
    #[arg(long)]
    model_size_bytes: u64,
    /// Conservative peak host-memory estimate for the intended context.
    #[arg(long)]
    estimated_peak_memory_bytes: u64,
    /// Select a human receipt or one machine-readable JSON value.
    #[arg(long, value_enum, default_value_t = CapacityFormat::Text)]
    format: CapacityFormat,
}

impl CapacityArgs {
    pub fn run(self, repo_root: PathBuf) -> Result<()> {
        let result = advisor_benchmark::check_capacity(&advisor_benchmark::CapacityCheckOptions {
            repo_root,
            model: self.model,
            model_size_bytes: self.model_size_bytes,
            estimated_peak_memory_bytes: self.estimated_peak_memory_bytes,
        })?;
        if self.format == CapacityFormat::Json {
            println!("{}", serde_json::to_string_pretty(&result.receipt)?);
        } else {
            println!("Advisor capacity check");
            println!("- eligible: {}", if result.eligible { "yes" } else { "no" });
            println!("- provider contacted: no");
            println!("- repository report accessed: no");
            println!(
                "- model: {}",
                result.receipt["model"].as_str().unwrap_or("unknown")
            );
            println!(
                "- model artifact: {} bytes",
                result.receipt["model_size_bytes"]
            );
            println!(
                "- minimum model artifact: {} bytes",
                result.receipt["limits"]["minimum_model_size_bytes"]
            );
            println!(
                "- estimated peak memory: {} bytes",
                result.receipt["estimated_peak_memory_bytes"]
            );
            println!(
                "- minimum estimated peak memory: {} bytes",
                result.receipt["limits"]["minimum_estimated_peak_memory_bytes"]
            );
            println!(
                "- physical memory: {} bytes",
                result.receipt["host"]["physical_memory_bytes"]
            );
            println!(
                "- available memory: {} bytes",
                result.receipt["host"]["available_memory_bytes"]
            );
            println!(
                "- current swap: {} bytes",
                result.receipt["host"]["swap_used_bytes"]
            );
            println!(
                "- required physical memory: {} bytes",
                result.receipt["required_physical_memory_bytes"]
            );
            println!(
                "- required available memory: {} bytes",
                result.receipt["required_available_memory_bytes"]
            );
            println!(
                "- maximum initial swap: {} bytes",
                result.receipt["limits"]["maximum_initial_swap_used_bytes"]
            );
            println!(
                "- maximum swap growth: {} bytes",
                result.receipt["limits"]["maximum_swap_growth_bytes"]
            );
            if let Some(blockers) = result.receipt["blockers"].as_array() {
                for blocker in blockers {
                    println!(
                        "- blocker [{}]: {}",
                        blocker["code"].as_str().unwrap_or("unknown"),
                        blocker["message"].as_str().unwrap_or("capacity rejected")
                    );
                }
            }
        }
        if result.eligible {
            Ok(())
        } else {
            anyhow::bail!("advisor capacity check rejected this host")
        }
    }
}

#[derive(Debug, Args)]
pub struct BenchmarkArgs {
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
    /// Explicit provider adapter used by the separately provisioned benchmark host.
    #[arg(long, value_parser = ["ollama", "openai-compatible"])]
    provider: Option<String>,
    /// Explicit loopback provider endpoint on the separately provisioned host.
    #[arg(long)]
    endpoint: Option<String>,
    /// Explicit canonical model identity.
    #[arg(long)]
    model: Option<String>,
    /// Explicit runtime-specific served model name.
    #[arg(long)]
    runtime_model: Option<String>,
    /// Exact runtime name and version.
    #[arg(long)]
    runtime_label: String,
    /// Exact model digest or immutable revision.
    #[arg(long)]
    model_digest: String,
    /// Exact model quantization reported by the runtime.
    #[arg(long)]
    model_quantization: Option<String>,
    /// Exact model artifact size in bytes.
    #[arg(long)]
    model_size_bytes: Option<u64>,
    /// Conservative peak memory estimate for model loading and the 16K context.
    #[arg(long)]
    estimated_peak_memory_bytes: Option<u64>,
    /// Confirm this is a dedicated, adequately resourced benchmark host.
    #[arg(long)]
    confirm_dedicated_host: bool,
    /// Explicit provider state before the first sample; the harness never changes it.
    #[arg(long, value_parser = ["cold", "warm"])]
    initial_runtime_state: Option<String>,
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
}

impl BenchmarkArgs {
    pub fn run(self, repo_root: PathBuf) -> Result<()> {
        let required = |value: Option<String>, flag: &str| -> Result<String> {
            value.ok_or_else(|| anyhow::anyhow!("inference runs require explicit {flag}"))
        };
        let (provider, endpoint, model, runtime_model, model_quantization, initial_runtime_state) =
            if self.prepare_only {
                (
                    "not-applicable".to_string(),
                    "not-applicable".to_string(),
                    "not-applicable".to_string(),
                    "not-applicable".to_string(),
                    "not-applicable".to_string(),
                    "not-applicable".to_string(),
                )
            } else {
                (
                    required(self.provider, "--provider")?,
                    required(self.endpoint, "--endpoint")?,
                    required(self.model, "--model")?,
                    required(self.runtime_model, "--runtime-model")?,
                    required(self.model_quantization, "--model-quantization")?,
                    required(self.initial_runtime_state, "--initial-runtime-state")?,
                )
            };
        let (results, decision) = advisor_benchmark::run(&advisor_benchmark::Options {
            repo_root,
            binary: self.binary,
            corpus: self.corpus,
            thresholds: self.thresholds,
            repositories: self.repositories,
            endpoint,
            provider,
            model,
            runtime_model,
            runtime_label: self.runtime_label,
            model_digest: self.model_digest,
            model_quantization,
            model_size_bytes: self.model_size_bytes,
            estimated_peak_memory_bytes: self.estimated_peak_memory_bytes,
            confirm_dedicated_host: self.confirm_dedicated_host,
            initial_runtime_state,
            output_dir: self.output_dir,
            repetitions: self.repetitions,
            full_matrix: self.full_matrix,
            prepare_only: self.prepare_only,
            ratings: self.ratings,
            review_output_dir: self.review_output_dir,
        })?;
        println!("Wrote advisor benchmark results: {}", results.display());
        println!("Wrote advisor benchmark decision: {}", decision.display());
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct FinalizeArgs {
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
}

impl FinalizeArgs {
    pub fn run(self, repo_root: &Path) -> Result<()> {
        let decision = advisor_benchmark::finalize(
            repo_root,
            &self.corpus,
            &self.thresholds,
            &self.results,
            &self.ratings,
        )?;
        println!(
            "Finalized advisor benchmark decision: {}",
            decision.display()
        );
        Ok(())
    }
}
