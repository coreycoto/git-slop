use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use git_slop_xtask::advisor_benchmark;

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
    /// Provider adapter used by the separately provisioned benchmark host.
    #[arg(long, value_parser = ["ollama", "openai-compatible"], default_value = "ollama")]
    provider: String,
    /// Provider endpoint; defaults to the selected adapter's loopback endpoint.
    #[arg(long)]
    endpoint: Option<String>,
    /// Runtime-specific served model name.
    #[arg(long, default_value = "gpt-oss-safeguard:20b")]
    runtime_model: String,
    /// Exact runtime name and version.
    #[arg(long)]
    runtime_label: String,
    /// Exact model digest or immutable revision.
    #[arg(long)]
    model_digest: String,
    /// Exact model quantization reported by the runtime.
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
}

impl BenchmarkArgs {
    pub fn run(self, repo_root: PathBuf) -> Result<()> {
        let endpoint = self.endpoint.unwrap_or_else(|| {
            if self.provider == "ollama" {
                "http://127.0.0.1:11434/api/chat".to_string()
            } else {
                "http://127.0.0.1:11434/v1/chat/completions".to_string()
            }
        });
        let (results, decision) = advisor_benchmark::run(&advisor_benchmark::Options {
            repo_root,
            binary: self.binary,
            corpus: self.corpus,
            thresholds: self.thresholds,
            repositories: self.repositories,
            endpoint,
            provider: self.provider,
            runtime_model: self.runtime_model,
            runtime_label: self.runtime_label,
            model_digest: self.model_digest,
            model_quantization: self.model_quantization,
            output_dir: self.output_dir,
            repetitions: self.repetitions,
            full_matrix: self.full_matrix,
            prepare_only: self.prepare_only,
            ratings: self.ratings,
            review_output_dir: self.review_output_dir,
            ollama_cold_model: self.ollama_cold_model,
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
