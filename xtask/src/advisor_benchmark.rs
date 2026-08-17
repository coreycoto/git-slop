use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const BENCHMARK_RUNTIME_CONTEXT_TOKENS: usize = 16_384;
const BENCHMARK_TIMEOUT_SECONDS: u64 = 600;
const BENCHMARK_CHILD_DEADLINE_SECONDS: u64 = BENCHMARK_TIMEOUT_SECONDS + 60;
const BENCHMARK_CONSECUTIVE_PROVIDER_FAILURE_LIMIT: usize = 2;
const BENCHMARK_CHILD_OUTPUT_LIMIT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Options {
    pub repo_root: PathBuf,
    pub binary: PathBuf,
    pub corpus: PathBuf,
    pub thresholds: PathBuf,
    pub repositories: Vec<String>,
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub runtime_model: String,
    pub runtime_label: String,
    pub model_digest: String,
    pub model_quantization: String,
    pub model_size_bytes: Option<u64>,
    pub estimated_peak_memory_bytes: Option<u64>,
    pub confirm_dedicated_host: bool,
    pub initial_runtime_state: String,
    pub output_dir: PathBuf,
    pub repetitions: usize,
    pub full_matrix: bool,
    pub prepare_only: bool,
    pub review_output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CapacityCheckOptions {
    pub repo_root: PathBuf,
    pub model: String,
    pub model_size_bytes: u64,
    pub estimated_peak_memory_bytes: u64,
}

pub struct CapacityCheck {
    pub receipt: Value,
    pub eligible: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CapacityBlocker {
    code: &'static str,
    message: String,
    actual_bytes: u64,
    comparison: &'static str,
    limit_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    schema_version: u64,
    id: String,
    license: String,
    review_status: String,
    privacy: String,
    repositories: BTreeMap<String, RepositoryFixture>,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryFixture {
    revision: String,
    as_of: String,
    expected_report_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    repository: String,
    scenario_tags: Vec<String>,
    selector: Vec<String>,
    scenario: String,
    candidate_count: usize,
    expected_aggregate: String,
    expected_rule_verdicts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Thresholds {
    schema_version: u64,
    preregistered_before_final_corpus: bool,
    structured_output_success_rate_minimum: f64,
    high_severity_rule_accuracy_minimum: f64,
    aggregate_verdict_accuracy_minimum: f64,
    citation_completeness_minimum: f64,
    repeated_verdict_consistency_minimum: f64,
    accepted_invalid_reference_maximum: u64,
    accepted_detector_truth_change_maximum: u64,
    abstention_recall_minimum: f64,
    maintainer_usefulness_mean_minimum: f64,
    manual_quality_mean_minimum: f64,
    unsupported_claim_count_maximum: u64,
    warm_top_one_p95_ms_maximum: u64,
    warm_top_five_p95_ms_maximum: u64,
    peak_process_rss_bytes_maximum: u64,
    swap_growth_bytes_maximum: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Sample {
    case_id: String,
    repository: String,
    scenario_tags: Vec<String>,
    scenario: String,
    candidate_count: usize,
    actual_candidate_count: Option<usize>,
    report_sha256: String,
    reasoning_effort: String,
    context_token_limit: usize,
    output_token_limit: usize,
    repetition: usize,
    phase: String,
    status: String,
    exit_code: Option<i32>,
    total_elapsed_ms: u128,
    peak_process_rss_bytes: Option<u64>,
    system_available_memory_before_bytes: Option<u64>,
    system_available_memory_after_bytes: Option<u64>,
    system_available_memory_minimum_bytes: Option<u64>,
    swap_before_bytes: Option<u64>,
    swap_after_bytes: Option<u64>,
    swap_growth_bytes: Option<u64>,
    context_elapsed_ms: Option<u64>,
    provider_elapsed_ms: Option<u64>,
    validation_elapsed_ms: Option<u64>,
    time_to_validated_artifact_ms: Option<u64>,
    model_load_duration_ns: Option<u64>,
    prompt_eval_duration_ns: Option<u64>,
    generation_duration_ns: Option<u64>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    prompt_tokens_per_second: Option<f64>,
    output_tokens_per_second: Option<f64>,
    reported_aggregate: Option<String>,
    expected_aggregate: String,
    aggregate_match: bool,
    matched_rule_verdicts: usize,
    expected_rule_verdicts: usize,
    accepted_invalid_references: u64,
    accepted_detector_truth_changes: u64,
    citation_complete: bool,
    retry_count: u64,
    failure_category: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RatingsFile {
    schema_version: u64,
    reviewer_count: usize,
    cases: BTreeMap<String, CaseRating>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseRating {
    recommendation_usefulness: f64,
    fact_interpretation_separation: f64,
    scope_quality: f64,
    verification_quality: f64,
    actionability: f64,
    unsupported_claim_count: u64,
}

#[derive(Debug, Serialize)]
struct ManualScores {
    reviewer_count: usize,
    recommendation_usefulness_mean: f64,
    fact_interpretation_separation_mean: f64,
    scope_quality_mean: f64,
    verification_quality_mean: f64,
    actionability_mean: f64,
    overall_quality_mean: f64,
    unsupported_claim_count: u64,
}

struct TemporaryWorkspace {
    path: PathBuf,
}

impl TemporaryWorkspace {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "git-slop-advisor-benchmark-{}-{}",
            std::process::id(),
            now_ms()
        ));
        if path.exists() {
            bail!("refusing to reuse benchmark temporary workspace");
        }
        fs::create_dir(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { path })
    }
}

impl Drop for TemporaryWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn validate_benchmark_result(value: &Value) -> Result<()> {
    let schema: Value =
        serde_json::from_str(include_str!("../../schemas/advisor-benchmark-1.json"))?;
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .context("embedded advisor benchmark schema is invalid")?;
    if let Some(error) = validator.iter_errors(value).next() {
        bail!(
            "advisor benchmark result does not match schema 1 at {}: {}",
            error.instance_path(),
            error
        );
    }
    Ok(())
}

include!("advisor_benchmark/corpus.rs");
include!("advisor_benchmark/io.rs");
include!("advisor_benchmark/system.rs");
include!("advisor_benchmark/provenance.rs");
include!("advisor_benchmark/scoring.rs");
include!("advisor_benchmark/artifact_validation.rs");
include!("advisor_benchmark/evidence.rs");
include!("advisor_benchmark/recommendation.rs");
include!("advisor_benchmark/persistence.rs");
include!("advisor_benchmark/decision.rs");
include!("advisor_benchmark/aggregate.rs");
include!("advisor_benchmark/preflight.rs");
include!("advisor_benchmark/finalization.rs");
include!("advisor_benchmark/run.rs");

fn release_matrix_complete(options: &Options) -> bool {
    options.full_matrix && options.repetitions >= 3
}

include!("advisor_benchmark/tests.rs");
