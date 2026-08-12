use std::fs;
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use serde_json::{Value, json};
use sha2::Digest;

use crate::build_info;
use crate::config;
use crate::error::{ClassifiedError, ErrorKind};
use crate::health;
use crate::report;
use crate::report_ops::{
    ExplainSelector, PlanSelector, PromptPackOptions, compare_payload_with_policy, explain_payload,
    failing_records_in, health_json_payload, plan_payload, render_compare_text,
    render_explain_text, render_github_annotations, render_json, render_plan_text,
    render_show_text, sarif_payload, show_payload, write_prompt_pack,
};
use crate::{PROJECT_NAME, VERSION, analyze, git};

#[derive(Debug, Parser)]
#[command(
    name = "git-slop",
    about = "Find the files that cost too much context.",
    version = VERSION
)]
struct Cli {
    /// Repository or path inside a repository to analyze.
    #[arg(long, global = true, value_name = "PATH")]
    repo: Option<PathBuf>,
    /// Render runtime errors as human text or stable JSON.
    #[arg(long, global = true, value_enum, default_value_t = ErrorFormat::Human)]
    error_format: ErrorFormat,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scaffold .slop/ config, ignore rules, and state directories.
    Init(InitArgs),
    /// Scan the repository and generate hotspot reports.
    Find(FindArgs),
    /// Show metrics and reasons for one file or folder.
    Show(ShowArgs),
    /// Explain why selected hotspots or structural findings are expensive.
    Explain(ExplainArgs),
    /// Propose bounded maintenance slices from the current detector report.
    Plan(PlanArgs),
    /// Evaluate an existing report against CI thresholds.
    Check(CheckArgs),
    /// Compare two existing schema-5 reports without rerunning the detector.
    Compare(CompareArgs),
    /// Manage named comparison baselines in Git-private runtime storage.
    Baseline(BaselineArgs),
    /// Validate or inspect the versioned report contract.
    Report(ReportArgs),
    /// Export action-queue findings from an existing schema-5 report as SARIF.
    Sarif(SarifArgs),
    /// Render repository health for CI summaries, annotations, or automation.
    Health(HealthArgs),
    /// Inspect or migrate effective configuration.
    Config(ConfigArgs),
    /// Diagnose repository readiness and optionally write a redacted bundle.
    Doctor(DoctorArgs),
    /// List findings, relationships, clusters, or profiles.
    List(ListArgs),
    /// Remove old immutable run snapshots according to retention policy.
    Prune(PruneArgs),
    /// Inspect or prune the packed token cache.
    Cache(CacheArgs),
    /// Generate shell completion source.
    Completions(CompletionsArgs),
    /// Generate the roff manual from the live Clap command tree.
    Man(ManArgs),
    /// Generate Markdown command reference from the live Clap command tree.
    Reference(ReferenceArgs),
    /// Write a self-contained, local, searchable HTML report.
    Html(HtmlArgs),
    /// Print version information.
    Version,
    /// Print package and source-build provenance.
    BuildInfo(BuildInfoArgs),
    /// Print a published JSON Schema for a machine contract.
    Schema(SchemaArgs),
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Init(_) => "init",
            Self::Find(_) => "find",
            Self::Show(_) => "show",
            Self::Explain(_) => "explain",
            Self::Plan(_) => "plan",
            Self::Check(_) => "check",
            Self::Compare(_) => "compare",
            Self::Baseline(_) => "baseline",
            Self::Report(_) => "report",
            Self::Sarif(_) => "sarif",
            Self::Health(_) => "health",
            Self::Config(_) => "config",
            Self::Doctor(_) => "doctor",
            Self::List(_) => "list",
            Self::Prune(_) => "prune",
            Self::Cache(_) => "cache",
            Self::Completions(_) => "completions",
            Self::Man(_) => "man",
            Self::Reference(_) => "reference",
            Self::Html(_) => "html",
            Self::Version => "version",
            Self::BuildInfo(_) => "build-info",
            Self::Schema(_) => "schema",
        }
    }
}

#[derive(Debug, Args)]
struct FindArgs {
    /// Acknowledge incomplete history and continue in a shallow clone.
    #[arg(long)]
    allow_shallow: bool,
    /// Analyze only this repo-relative path while retaining repository-wide Git evidence.
    #[arg(long)]
    scope: Option<String>,
    /// Permit a scope that selects no tracked paths and emit an empty analysis.
    #[arg(long)]
    allow_empty_scope: bool,
    /// Suppress human progress and report-path messages.
    #[arg(long)]
    quiet: bool,
    /// Suppress phase progress while preserving the final result.
    #[arg(long)]
    no_progress: bool,
    /// Mutable cache/state directory. Relative paths resolve from the repository root.
    #[arg(long, value_name = "PATH")]
    state_dir: Option<PathBuf>,
    /// Report output directory. Relative paths resolve from the repository root.
    #[arg(long, value_name = "PATH")]
    output_dir: Option<PathBuf>,
    /// Disable token-cache reads and writes for an ephemeral scan.
    #[arg(long)]
    no_cache: bool,
    /// Deterministically analyze the largest path prefix that fits the memory budget.
    #[arg(long)]
    allow_degraded: bool,
    /// Fixed RFC 3339 analysis clock for reproducible recency and history windows.
    #[arg(long, value_name = "RFC3339")]
    as_of: Option<String>,
    /// Report evidence profile.
    #[arg(long, value_enum, default_value_t = ReportProfile::Standard)]
    report_profile: ReportProfile,
    /// Also write a compressed report beside report.json.
    #[arg(long, value_enum, default_value_t = ReportCompression::None)]
    compression: ReportCompression,
    /// Estimate scope, memory, cache, report size, time, and inodes without scanning.
    #[arg(long)]
    estimate_only: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportProfile {
    Compact,
    Standard,
    FullEvidence,
}

impl ReportProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Standard => "standard",
            Self::FullEvidence => "full_evidence",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportCompression {
    None,
    Gzip,
    Zstd,
}

impl ReportCompression {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
        }
    }
}

#[derive(Debug, Args)]
struct BuildInfoArgs {
    /// Machine-readable build provenance format.
    #[arg(long, value_enum, default_value_t = BuildInfoFormat::Json)]
    format: BuildInfoFormat,
}

#[derive(Debug, Args)]
struct InitArgs {
    /// Overwrite generated config files.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct ShowArgs {
    /// Repo-relative file or folder path.
    target_path: String,
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("selector")
        .args(["path", "cluster", "relationship", "top"])
        .multiple(false)
))]
struct ExplainArgs {
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Repo-relative file or folder path.
    #[arg(long)]
    path: Option<String>,
    /// Cluster identifier.
    #[arg(long)]
    cluster: Option<String>,
    /// Relationship identifier.
    #[arg(long)]
    relationship: Option<String>,
    /// Explain the top N hotspots from the action queue.
    #[arg(long)]
    top: Option<i64>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
    /// Write a deterministic local-model prompt pack to this directory.
    #[arg(long)]
    prompt_pack: Option<PathBuf>,
    /// Atomically replace an existing prompt-pack directory.
    #[arg(long, requires = "prompt_pack")]
    force: bool,
    /// Include bounded local source/test excerpts, guidance, and verification hints.
    #[arg(long, requires = "prompt_pack")]
    include_repository_context: bool,
    /// Maximum bytes read from each included repository file.
    #[arg(long, default_value_t = 2048, requires = "include_repository_context")]
    excerpt_bytes: usize,
    /// Include local filesystem paths in prompt-pack provenance and commands.
    #[arg(long, requires = "prompt_pack")]
    include_local_paths: bool,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("selector")
        .args(["path", "cluster", "relationship"])
        .required(true)
        .multiple(false)
))]
struct PlanArgs {
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Repo-relative file or folder path.
    #[arg(long)]
    path: Option<String>,
    /// Cluster identifier.
    #[arg(long)]
    cluster: Option<String>,
    /// Relationship identifier.
    #[arg(long)]
    relationship: Option<String>,
    /// Maximum number of bounded maintenance slices to propose.
    #[arg(long, default_value_t = 3)]
    max_slices: i64,
    /// Output format.
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
    /// Write a deterministic local-model prompt pack to this directory.
    #[arg(long)]
    prompt_pack: Option<PathBuf>,
    /// Atomically replace an existing prompt-pack directory.
    #[arg(long, requires = "prompt_pack")]
    force: bool,
    /// Include bounded local source/test excerpts, guidance, and verification hints.
    #[arg(long, requires = "prompt_pack")]
    include_repository_context: bool,
    /// Maximum bytes read from each included repository file.
    #[arg(long, default_value_t = 2048, requires = "include_repository_context")]
    excerpt_bytes: usize,
    /// Include local filesystem paths in prompt-pack provenance and commands.
    #[arg(long, requires = "prompt_pack")]
    include_local_paths: bool,
}

#[derive(Debug, Args)]
struct CheckArgs {
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Override the config default fail threshold for context_band.
    #[arg(long, value_enum)]
    fail_on_context_band: Option<ContextBand>,
    /// Override the config default fail threshold for slop_band.
    #[arg(long, value_enum)]
    fail_on_slop_band: Option<SlopBand>,
    /// Output format, including escaped GitHub workflow commands.
    #[arg(long, value_enum, default_value_t = CheckFormat::Text)]
    format: CheckFormat,
    /// Include complete finding records in JSON output.
    #[arg(long)]
    details: bool,
    /// Include folder records in addition to the versioned file-only gate.
    #[arg(long)]
    include_folders: bool,
    /// Zero-based finding offset used with --details.
    #[arg(long, default_value_t = 0, requires = "details")]
    offset: usize,
    /// Maximum finding records returned with --details.
    #[arg(long, default_value_t = 1000, requires = "details")]
    limit: usize,
    /// Permit policy evaluation when selected inventory records are incomplete.
    #[arg(long)]
    allow_incomplete_evidence: bool,
    /// Evaluate and report the canonical policy result without returning exit 1 for findings.
    #[arg(long)]
    evaluate_only: bool,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Mutable state directory for named baselines. Relative paths resolve from the repository root.
    #[arg(long, value_name = "PATH")]
    state_dir: Option<PathBuf>,
    /// Base report.json path.
    #[arg(
        long,
        required_unless_present_any = ["base_ref", "baseline"],
        conflicts_with_all = ["base_ref", "baseline"]
    )]
    base: Option<PathBuf>,
    /// Safely resolve and scan this Git revision in an isolated worktree.
    #[arg(long, conflicts_with_all = ["base", "baseline"])]
    base_ref: Option<String>,
    /// Use a named baseline from Git-private runtime storage.
    #[arg(long, conflicts_with_all = ["base", "base_ref"])]
    baseline: Option<String>,
    /// Head report.json path.
    #[arg(long, default_value = ".slop/latest/report.json")]
    head: PathBuf,
    /// Apply the head repository's scope to an isolated --base-ref scan.
    #[arg(long)]
    scope: Option<String>,
    /// Permit incomplete history in an isolated --base-ref scan.
    #[arg(long)]
    allow_shallow: bool,
    /// Permit comparison when selected inventory records are incomplete.
    #[arg(long)]
    allow_incomplete_evidence: bool,
    /// Maximum number of changed files and queue movements to show.
    #[arg(long, default_value_t = 10)]
    top: i64,
    /// Output format.
    #[arg(long, value_enum, default_value_t = CompareFormat::Text)]
    format: CompareFormat,
    /// Detail level for machine output.
    #[arg(long, value_enum, default_value_t = CompareDetail::Top)]
    detail: CompareDetail,
    /// Zero-based record offset for --detail full.
    #[arg(long, default_value_t = 0)]
    offset: usize,
    /// Maximum records per collection for --detail full.
    #[arg(long, default_value_t = 1000)]
    limit: usize,
    /// Compare reports with incompatible identity or analyzer metadata.
    #[arg(long)]
    force: bool,
    /// Include local filesystem report paths in output descriptors.
    #[arg(long)]
    include_local_paths: bool,
    /// Include unchanged file and folder records in bounded compare collections.
    #[arg(long)]
    include_unchanged: bool,
    /// Select which report supplies regression thresholds and evidence-drift policy.
    #[arg(long, value_enum, default_value_t = PolicySource::Base)]
    policy_from: PolicySource,
    /// Exit 1 when an existing file worsens or a newly added file is a finding.
    #[arg(long)]
    fail_on_regression: bool,
}

#[derive(Debug, Args)]
struct BaselineArgs {
    /// Mutable state directory. Relative paths resolve from the repository root.
    #[arg(long, value_name = "PATH", global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: BaselineCommand,
}

#[derive(Debug, Subcommand)]
enum BaselineCommand {
    /// Idempotently save a named baseline, failing closed when stored content differs.
    Ensure {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
        /// Report path. Defaults to .slop/latest/report.json.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Explicitly replace a differing stored baseline.
        #[arg(long)]
        replace: bool,
        /// Permit a report produced from a dirty worktree.
        #[arg(long)]
        allow_dirty: bool,
        /// Permit incomplete inventory or history evidence.
        #[arg(long)]
        allow_incomplete_evidence: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// Create a named baseline from a validated report.
    Create {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
        /// Report path. Defaults to .slop/latest/report.json.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Replace an existing named baseline.
        #[arg(long)]
        force: bool,
        /// Permit a report produced from a dirty worktree.
        #[arg(long)]
        allow_dirty: bool,
        /// Permit incomplete inventory or history evidence.
        #[arg(long)]
        allow_incomplete_evidence: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// Replace an existing named baseline from a validated report.
    Update {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
        /// Report path. Defaults to .slop/latest/report.json.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Permit a report produced from a dirty worktree.
        #[arg(long)]
        allow_dirty: bool,
        /// Permit incomplete inventory or history evidence.
        #[arg(long)]
        allow_incomplete_evidence: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// List named baselines with identity and readiness metadata.
    List {
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// Inspect baseline identity and evidence status.
    Inspect {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// Validate a named baseline against the current report contract.
    Validate {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// Remove a named baseline.
    Remove {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
}

#[derive(Debug, Args)]
struct ReportArgs {
    #[command(subcommand)]
    command: ReportCommand,
}

#[derive(Debug, Subcommand)]
enum ReportCommand {
    /// Validate one report against the complete schema-5 contract.
    Validate {
        /// Report JSON to validate.
        #[arg(value_name = "REPORT_JSON", required_unless_present = "report")]
        path: Option<PathBuf>,
        /// Report JSON to validate (alias for the positional path).
        #[arg(long, value_name = "REPORT_JSON", required_unless_present = "path")]
        report: Option<PathBuf>,
        /// Accept schema 4 as migration input and validate its normalized schema-5 form.
        #[arg(long)]
        allow_legacy: bool,
        /// Success output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// Migrate a schema-4 report to normalized schema 5.
    Migrate {
        /// Legacy report to migrate.
        #[arg(value_name = "REPORT_JSON")]
        path: PathBuf,
        /// Destination for the normalized schema-5 report.
        #[arg(long, value_name = "PATH")]
        output: PathBuf,
    },
    /// Print the published JSON Schema for report schema 5.
    Schema,
}

#[derive(Debug, Args)]
struct SarifArgs {
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Maximum number of action-queue findings to export.
    #[arg(long)]
    top: Option<i64>,
    /// Export configured policy failures or action-queue intervention candidates.
    #[arg(long, value_enum, default_value_t = SarifScope::ActionQueue)]
    scope: SarifScope,
    /// Optional SARIF output path. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Include the local source report path in SARIF invocation properties.
    #[arg(long)]
    include_local_paths: bool,
}

#[derive(Debug, Args)]
struct HealthArgs {
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Output suited for a job summary, workflow annotations, or automation.
    #[arg(long, value_enum, default_value_t = HealthFormat::Text)]
    format: HealthFormat,
    /// Maximum number of GitHub workflow annotations to emit.
    #[arg(long, default_value_t = 10)]
    max_annotations: usize,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Show configuration; --effective includes defaults.
    Show {
        /// Include defaults after applying repository overrides.
        #[arg(long)]
        effective: bool,
    },
    /// Validate the local configuration.
    Validate,
    /// Show only values that differ from defaults.
    DiffDefaults,
    /// Rewrite legacy schema configuration as a minimal schema-2 override.
    Migrate,
    /// Print the supported configuration schema as JSON.
    Schema,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    /// Write a privacy-safe diagnostic JSON bundle.
    #[arg(long, num_args = 0..=1, default_missing_value = ".slop/diagnostic-bundle.json")]
    bundle: Option<PathBuf>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = DoctorFormat::Text)]
    format: DoctorFormat,
    /// Estimate only this repo-relative scope.
    #[arg(long)]
    scope: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DoctorFormat {
    Text,
    Json,
}

#[derive(Debug, Args)]
struct ListArgs {
    #[command(subcommand)]
    command: ListCommand,
}

#[derive(Debug, Subcommand)]
enum ListCommand {
    Findings(ListFilterArgs),
    Relationships(ListFilterArgs),
    Clusters(ListFilterArgs),
    Profiles(ListFilterArgs),
}

#[derive(Debug, Args)]
struct ListFilterArgs {
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Match a finding path, relationship endpoint, or cluster member.
    #[arg(long)]
    path: Option<String>,
    /// Match an analysis profile.
    #[arg(long)]
    profile: Option<String>,
    /// Match a resolved file language.
    #[arg(long)]
    language: Option<String>,
    /// Match a resolved file classification.
    #[arg(long, visible_alias = "class")]
    classification: Option<String>,
    /// Match a finding severity.
    #[arg(long)]
    severity: Option<String>,
    /// Maximum number of matched records to return.
    #[arg(long, default_value_t = 50)]
    top: usize,
    /// Output format.
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
    /// Use a wider terminal layout before truncating fields.
    #[arg(long)]
    wide: bool,
    /// Never truncate terminal fields.
    #[arg(long)]
    no_truncate: bool,
}

#[derive(Debug, Args)]
struct PruneArgs {
    /// Number of newest run snapshots to retain; defaults to output.retention_runs.
    #[arg(long)]
    keep: Option<usize>,
    /// Maximum total bytes retained; defaults to output.retention_bytes.
    #[arg(long)]
    max_bytes: Option<u64>,
    /// Print removals without changing files.
    #[arg(long)]
    dry_run: bool,
    /// Select text, JSON, or YAML output.
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
}

#[derive(Debug, Args)]
struct CacheArgs {
    /// Mutable state directory. Defaults to Git-private runtime storage.
    #[arg(long, value_name = "PATH", global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: CacheCommand,
}

#[derive(Debug, Subcommand)]
enum CacheCommand {
    Status {
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    Prune {
        /// Maximum entries to retain.
        #[arg(long, default_value_t = 10_000)]
        max_entries: usize,
        /// Maximum logical payload bytes to retain.
        #[arg(long, default_value_t = 536_870_912)]
        max_bytes: u64,
        /// Preview cache removals without changing the database.
        #[arg(long)]
        dry_run: bool,
        /// Reclaim free database pages after pruning.
        #[arg(long)]
        compact: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
}

#[derive(Debug, Args)]
struct CompletionsArgs {
    /// Shell whose completion source should be generated.
    shell: CompletionShell,
}

#[derive(Debug, Args)]
struct ManArgs {
    /// Destination file. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ReferenceArgs {
    /// Destination file. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct HtmlArgs {
    /// Report path. Defaults to .slop/latest/report.json.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Destination. Defaults to .slop/latest/report.html.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Embed the local source report path in the otherwise portable HTML file.
    #[arg(long)]
    include_local_paths: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Nushell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DisplayFormat {
    Text,
    Json,
    Yaml,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompareFormat {
    Text,
    Json,
    Yaml,
    Ndjson,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompareDetail {
    Summary,
    Top,
    Full,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HealthFormat {
    Text,
    Markdown,
    Github,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CheckFormat {
    Text,
    Json,
    Github,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PolicySource {
    Base,
    Head,
}

impl PolicySource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Head => "head",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ErrorFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ContextBand {
    Compact,
    Healthy,
    Warning,
    Critical,
}

impl ContextBand {
    fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Healthy => "healthy",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SlopBand {
    Low,
    Moderate,
    High,
    Critical,
}

impl SlopBand {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Moderate => "moderate",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

include!("cli/support.rs");
include!("cli/analysis.rs");
include!("cli/check.rs");
include!("cli/baseline_compare.rs");
include!("cli/reporting.rs");
include!("cli/listing.rs");
include!("cli/generation.rs");
fn execute(repo_root: &Path, command: Command) -> Result<i32> {
    match command {
        Command::Init(args) => run_init(repo_root, args),
        Command::Find(args) => run_find(repo_root, args),
        Command::Show(args) => run_show(repo_root, args),
        Command::Explain(args) => run_explain(repo_root, args),
        Command::Plan(args) => run_plan(repo_root, args),
        Command::Check(args) => run_check(repo_root, args),
        Command::Compare(args) => run_compare(repo_root, args),
        Command::Baseline(args) => run_baseline(repo_root, args),
        Command::Report(args) => run_report(args),
        Command::Sarif(args) => run_sarif(repo_root, args),
        Command::Health(args) => run_health(repo_root, args),
        Command::Config(args) => run_config(repo_root, args),
        Command::Doctor(args) => run_doctor(repo_root, args),
        Command::List(args) => run_list(repo_root, args),
        Command::Prune(args) => run_prune(repo_root, args),
        Command::Cache(args) => run_cache(repo_root, args),
        Command::Completions(args) => run_completions(args),
        Command::Man(args) => run_man(args),
        Command::Reference(args) => run_reference(args),
        Command::Html(args) => run_html(repo_root, args),
        Command::Version => {
            println!("{PROJECT_NAME} {VERSION}");
            Ok(0)
        }
        Command::BuildInfo(args) => {
            match args.format {
                BuildInfoFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&build_info::current())?)
                }
            }
            Ok(0)
        }
        Command::Schema(args) => {
            let rendered = match args.contract {
                SchemaContract::Report => render_json(&report::schema())?,
                SchemaContract::Config => render_json(&config::schema())?,
                contract => {
                    let source = match contract {
                        SchemaContract::Compare => include_str!("../schemas/compare-1.json"),
                        SchemaContract::Explain => include_str!("../schemas/explain-2.json"),
                        SchemaContract::Plan => include_str!("../schemas/plan-2.json"),
                        SchemaContract::Sarif => include_str!("../schemas/sarif-1.json"),
                        SchemaContract::Health => include_str!("../schemas/health-1.json"),
                        SchemaContract::Check => include_str!("../schemas/check-1.json"),
                        SchemaContract::Doctor => include_str!("../schemas/doctor-1.json"),
                        SchemaContract::BuildInfo => include_str!("../schemas/build-info-2.json"),
                        SchemaContract::ReleaseManifest => {
                            include_str!("../schemas/release-manifest-3.json")
                        }
                        SchemaContract::List => include_str!("../schemas/list-1.json"),
                        SchemaContract::Show => include_str!("../schemas/show-1.json"),
                        SchemaContract::PromptManifest => {
                            include_str!("../schemas/prompt-manifest-1.json")
                        }
                        SchemaContract::Error => include_str!("../schemas/error-1.json"),
                        SchemaContract::FindEstimate => {
                            include_str!("../schemas/find-estimate-1.json")
                        }
                        SchemaContract::CacheStatus => {
                            include_str!("../schemas/cache-status-1.json")
                        }
                        SchemaContract::CachePrune => include_str!("../schemas/cache-prune-1.json"),
                        SchemaContract::Baseline => include_str!("../schemas/baseline-1.json"),
                        SchemaContract::Prune => include_str!("../schemas/prune-1.json"),
                        SchemaContract::CompareNdjson => {
                            include_str!("../schemas/compare-ndjson-1.json")
                        }
                        SchemaContract::Report | SchemaContract::Config => unreachable!(),
                    };
                    let value: Value = serde_json::from_str(source)?;
                    render_json(&value)?
                }
            };
            write_generated_output(args.output.as_deref(), rendered.as_bytes())?;
            Ok(0)
        }
    }
}

fn command_requires_repository(command: &Command) -> bool {
    match command {
        Command::Completions(_)
        | Command::Man(_)
        | Command::Reference(_)
        | Command::Version
        | Command::BuildInfo(_)
        | Command::Schema(_)
        | Command::Report(_)
        | Command::Config(ConfigArgs {
            command: ConfigCommand::Schema,
        }) => false,
        Command::Show(args) => args.report.is_none(),
        Command::Compare(args) => args.base_ref.is_some() || args.baseline.is_some(),
        Command::Baseline(_) => true,
        Command::Explain(args) => args.report.is_none() || args.include_repository_context,
        Command::Plan(args) => args.report.is_none() || args.include_repository_context,
        Command::Check(args) => args.report.is_none(),
        Command::Sarif(args) => args.report.is_none(),
        Command::Health(args) => args.report.is_none(),
        Command::Html(args) => args.report.is_none(),
        Command::List(ListArgs {
            command:
                ListCommand::Findings(args)
                | ListCommand::Relationships(args)
                | ListCommand::Clusters(args)
                | ListCommand::Profiles(args),
        }) => args.report.is_none(),
        _ => true,
    }
}

pub fn run() -> i32 {
    let raw_args = std::env::args_os().collect::<Vec<_>>();
    let requested_error_format = requested_error_format(&raw_args);
    let parser_command = parser_command_name(&raw_args);
    let cli = match Cli::try_parse_from(&raw_args) {
        Ok(cli) => cli,
        Err(error) => {
            let code = error.exit_code();
            if code == 0 || requested_error_format == ErrorFormat::Human {
                let _ = error.print();
            } else {
                let classified =
                    ClassifiedError::new(ErrorKind::Contract, "parser_error", error.to_string())
                        .at("/arguments")
                        .with_details(json!({"clap_kind": format!("{:?}", error.kind())}));
                render_runtime_error(
                    requested_error_format,
                    &classified,
                    parser_command.as_deref(),
                );
            }
            return code;
        }
    };
    let error_format = cli.error_format;
    let command_name = cli.command.name();
    let repo_root = if command_requires_repository(&cli.command) {
        match git::resolve_repo_root_from(cli.repo.as_deref()) {
            Ok(root) => root,
            Err(error) => {
                render_runtime_error(
                    error_format,
                    &ClassifiedError::new(
                        ErrorKind::Repository,
                        "repository_not_found",
                        format!("{error:#}"),
                    ),
                    Some(command_name),
                );
                return 3;
            }
        }
    } else {
        PathBuf::new()
    };
    match execute(&repo_root, cli.command) {
        Ok(code) => code,
        Err(error) => {
            let classified = error.downcast_ref::<ClassifiedError>();
            let fallback;
            let classified = if let Some(classified) = classified {
                classified
            } else {
                fallback = if error.downcast_ref::<std::io::Error>().is_some() {
                    ClassifiedError::new(ErrorKind::Io, "io_failure", format!("{error:#}"))
                } else {
                    ClassifiedError::new(
                        ErrorKind::Repository,
                        "operation_failed",
                        format!("{error:#}"),
                    )
                };
                &fallback
            };
            render_runtime_error(error_format, classified, Some(command_name));
            classified.kind.exit_code()
        }
    }
}

fn requested_error_format(args: &[std::ffi::OsString]) -> ErrorFormat {
    args.iter()
        .filter_map(|arg| arg.to_str())
        .enumerate()
        .find_map(|(index, arg)| {
            if arg == "--error-format" {
                args.get(index + 1).and_then(|value| value.to_str())
            } else {
                arg.strip_prefix("--error-format=")
            }
        })
        .filter(|value| *value == "json")
        .map_or(ErrorFormat::Human, |_| ErrorFormat::Json)
}

fn parser_command_name(args: &[std::ffi::OsString]) -> Option<String> {
    const COMMANDS: &[&str] = &[
        "init",
        "find",
        "show",
        "explain",
        "plan",
        "check",
        "compare",
        "baseline",
        "report",
        "sarif",
        "health",
        "config",
        "doctor",
        "list",
        "prune",
        "cache",
        "completions",
        "man",
        "reference",
        "html",
        "version",
        "build-info",
        "schema",
    ];
    args.iter()
        .skip(1)
        .filter_map(|arg| arg.to_str())
        .find(|arg| COMMANDS.contains(arg))
        .map(str::to_string)
}

fn render_runtime_error(format: ErrorFormat, error: &ClassifiedError, command: Option<&str>) {
    match format {
        ErrorFormat::Human => eprintln!("{}", error.message),
        ErrorFormat::Json => eprintln!(
            "{}",
            serde_json::to_string(&json!({
                "schema_version": 1,
                "error": {
                    "kind": error.kind,
                    "code": error.code,
                    "pointer": error.pointer,
                    "message": error.message,
                    "details": error.details,
                    "command": command,
                    "exit_code": error.kind.exit_code()
                }
            }))
            .unwrap_or_else(|_| {
                "{\"schema_version\":1,\"error\":{\"code\":\"serialization_failed\"}}".to_string()
            })
        ),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    fn assert_descriptions(command: &clap::Command, path: &str) {
        for argument in command.get_arguments() {
            assert!(
                argument
                    .get_help()
                    .is_some_and(|help| !help.to_string().trim().is_empty()),
                "{path} argument {} has no generated reference description",
                argument.get_id()
            );
        }
        for subcommand in command.get_subcommands() {
            assert_descriptions(subcommand, &format!("{path} {}", subcommand.get_name()));
        }
    }

    #[test]
    fn generated_reference_has_no_blank_argument_descriptions() {
        assert_descriptions(&Cli::command(), "git-slop");
    }
}
