use std::fs;
use std::io::IsTerminal;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
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
    ExplainSelector, PlanSelector, compare_payload_with_policy, explain_payload,
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
struct SchemaArgs {
    /// Machine contract whose immutable schema should be printed.
    #[arg(value_enum)]
    contract: SchemaContract,
    /// Destination file. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaContract {
    Report,
    Config,
    Compare,
    Explain,
    Plan,
    Sarif,
    Health,
    Check,
    Doctor,
    BuildInfo,
    List,
    Show,
    PromptManifest,
    Error,
    FindEstimate,
    CacheStatus,
    CachePrune,
    Prune,
    CompareNdjson,
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
}

#[derive(Debug, Args)]
struct CompareArgs {
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
    /// Select which report supplies regression thresholds and evidence-drift policy.
    #[arg(long, value_enum, default_value_t = PolicySource::Base)]
    policy_from: PolicySource,
    /// Exit 1 when an existing file worsens or a newly added file is a finding.
    #[arg(long)]
    fail_on_regression: bool,
}

#[derive(Debug, Args)]
struct BaselineArgs {
    #[command(subcommand)]
    command: BaselineCommand,
}

#[derive(Debug, Subcommand)]
enum BaselineCommand {
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
    },
    /// Replace an existing named baseline from a validated report.
    Update {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
        /// Report path. Defaults to .slop/latest/report.json.
        #[arg(long)]
        report: Option<PathBuf>,
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
    },
    /// Remove a named baseline.
    Remove {
        /// Stable baseline name.
        #[arg(long, default_value = "default")]
        name: String,
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
        #[arg(value_name = "REPORT_JSON")]
        path: PathBuf,
        /// Accept schema 4 as migration input and validate its normalized schema-5 form.
        #[arg(long)]
        allow_legacy: bool,
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
    /// Optional SARIF output path. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
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
    #[arg(long, value_name = "PATH")]
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
enum BuildInfoFormat {
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

fn print_text(value: &str) {
    if value.is_empty() {
        return;
    }
    print!("{value}");
    if !value.ends_with('\n') {
        println!();
    }
}

fn safe_terminal(value: &str) -> String {
    crate::text::visible_controls(value)
}

fn relative_display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn default_report_path(repo_root: &Path) -> PathBuf {
    config::latest_dir(repo_root).join("report.json")
}

fn load_report_at(path: &Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    report::load_report(path).map(Some)
}

fn load_default_report(
    repo_root: &Path,
    explicit_report: Option<&Path>,
) -> Result<Option<(Value, PathBuf)>> {
    let path = explicit_report
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_report_path(repo_root));
    Ok(load_report_at(&path)?.map(|report| (report, path)))
}

fn report_or_missing(repo_root: &Path, explicit_report: Option<&Path>) -> Result<(Value, PathBuf)> {
    let fallback = explicit_report
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_report_path(repo_root));
    let loaded = load_default_report(repo_root, explicit_report).map_err(|error| {
        ClassifiedError::new(ErrorKind::Contract, "report_invalid", format!("{error:#}"))
            .at("/report")
            .with_details(json!({"path": fallback}))
    })?;
    loaded.ok_or_else(|| {
        ClassifiedError::new(
            ErrorKind::Contract,
            "report_not_found",
            format!(
                "Report not found: {}\nRun `git slop find` to generate it.",
                fallback.display()
            ),
        )
        .at("/report")
        .with_details(json!({"path": fallback}))
        .into()
    })
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push(component.as_os_str());
                }
            }
            _ => result.push(component.as_os_str()),
        }
    }
    result
}

fn selector_path(repo_root: &Path, input: &str) -> String {
    let candidate = if Path::new(input).is_absolute() {
        lexical_normalize(Path::new(input))
    } else {
        lexical_normalize(&repo_root.join(input))
    };
    candidate
        .strip_prefix(repo_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                ".".to_string()
            } else {
                trimmed.to_string()
            }
        })
}

fn usage_error(error: impl std::fmt::Display) -> Result<i32> {
    Err(ClassifiedError::new(ErrorKind::Contract, "invalid_argument", error).into())
}

fn ensure_prompt_pack_target(path: &Path) -> Result<()> {
    if path.exists() && !path.is_dir() {
        Err(ClassifiedError::new(
            ErrorKind::Contract,
            "prompt_pack_collision",
            format!("Prompt pack path is not a directory: {}", path.display()),
        )
        .at("/prompt_pack")
        .with_details(json!({"path": path}))
        .into())
    } else {
        Ok(())
    }
}

fn run_init(repo_root: &Path, args: InitArgs) -> Result<i32> {
    let result = config::initialize(repo_root, args.force)?;
    println!(
        "Initialized {} ({}).",
        relative_display(&config::config_path(repo_root), repo_root),
        result.config
    );
    println!(
        "Initialized {} ({}).",
        relative_display(&config::slop_dir(repo_root).join(".gitignore"), repo_root),
        result.gitignore
    );
    println!("Ensured .slop/latest/, .slop/runs/, and .slop/cache/ exist.");
    Ok(0)
}

fn run_find(repo_root: &Path, args: FindArgs) -> Result<i32> {
    if args.estimate_only {
        let config = config::load(repo_root)?;
        let scope = args
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != ".");
        let paths = git::list_tracked_files(repo_root)?
            .into_iter()
            .filter(|path| {
                scope.is_none_or(|scope| path == scope || path.starts_with(&format!("{scope}/")))
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "schema_version": 1,
            "command": "find estimate",
            "scope": scope,
            "estimate": crate::estimate::build(repo_root, &paths, &config)
        });
        print_text(&render_json(&payload)?);
        return Ok(0);
    }
    let as_of = args
        .as_of
        .as_deref()
        .map(chrono::DateTime::parse_from_rfc3339)
        .transpose()
        .context("--as-of must be an RFC 3339 timestamp")?
        .map(|value| value.with_timezone(&chrono::Utc));
    let result = analyze::run_find_with_options(
        repo_root,
        &analyze::FindOptions {
            allow_shallow: args.allow_shallow,
            scope: args.scope,
            progress: !args.quiet && !args.no_progress && std::io::stderr().is_terminal(),
            allow_empty_scope: args.allow_empty_scope,
            state_dir: args.state_dir,
            output_dir: args.output_dir,
            no_cache: args.no_cache,
            allow_degraded: args.allow_degraded,
            as_of,
            report_profile: args.report_profile.as_str().to_string(),
            compression: args.compression.as_str().to_string(),
        },
    )?;
    if args.quiet {
        return Ok(0);
    }
    print_text(&result.terminal);
    println!("Wrote report to {}.", result.report_json.display());
    if result.report_yaml.exists() {
        println!("Wrote YAML report to {}.", result.report_yaml.display());
    }
    println!("Wrote summary to {}.", result.summary_md.display());
    println!(
        "Wrote repository health summary to {}.",
        result.health_md.display()
    );
    if let Some(path) = result.compressed_report {
        println!("Wrote compressed report to {}.", path.display());
    }
    Ok(0)
}

fn run_show(repo_root: &Path, args: ShowArgs) -> Result<i32> {
    let (loaded, report_path) = report_or_missing(repo_root, args.report.as_deref())?;
    let target = selector_path(repo_root, &args.target_path);
    let Some(payload) = show_payload(&loaded, &target) else {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "selector_not_found",
            format!(
                "No record found for '{}' in {}.",
                args.target_path,
                report_path.display()
            ),
        )
        .at("/target_path")
        .with_details(json!({"selector": args.target_path, "report": report_path}))
        .into());
    };
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_show_text(&payload)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn explain_selector(args: &ExplainArgs, repo_root: &Path) -> Result<ExplainSelector> {
    if let Some(path) = &args.path {
        Ok(ExplainSelector::Path(selector_path(repo_root, path)))
    } else if let Some(id) = &args.cluster {
        Ok(ExplainSelector::Cluster(id.clone()))
    } else if let Some(id) = &args.relationship {
        Ok(ExplainSelector::Relationship(id.clone()))
    } else {
        let count = args.top.unwrap_or(5);
        match usize::try_from(count).ok().filter(|count| *count > 0) {
            Some(count) => Ok(ExplainSelector::Top(count)),
            None => Err(ClassifiedError::new(
                ErrorKind::Contract,
                "invalid_argument",
                "--top must be greater than zero",
            )
            .at("/top")
            .into()),
        }
    }
}

fn run_explain(repo_root: &Path, args: ExplainArgs) -> Result<i32> {
    if args.include_repository_context && !(256..=4096).contains(&args.excerpt_bytes) {
        return usage_error("--excerpt-bytes must be between 256 and 4096");
    }
    let (loaded, _) = report_or_missing(repo_root, args.report.as_deref())?;
    let selector = explain_selector(&args, repo_root)?;
    let payload = match explain_payload(&loaded, Some(selector)) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    if let Some(output_dir) = args.prompt_pack.as_deref() {
        ensure_prompt_pack_target(output_dir)?;
        write_prompt_pack(
            "explain",
            &payload,
            &loaded,
            output_dir,
            args.include_repository_context.then_some(repo_root),
            args.excerpt_bytes,
            args.force,
        )?;
    }
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_explain_text(&payload)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn plan_selector(args: &PlanArgs, repo_root: &Path) -> PlanSelector {
    if let Some(path) = &args.path {
        PlanSelector::Path(selector_path(repo_root, path))
    } else if let Some(id) = &args.cluster {
        PlanSelector::Cluster(id.clone())
    } else {
        PlanSelector::Relationship(args.relationship.clone().unwrap_or_default())
    }
}

fn run_plan(repo_root: &Path, args: PlanArgs) -> Result<i32> {
    if args.include_repository_context && !(256..=4096).contains(&args.excerpt_bytes) {
        return usage_error("--excerpt-bytes must be between 256 and 4096");
    }
    let (loaded, _) = report_or_missing(repo_root, args.report.as_deref())?;
    let Some(max_slices) = usize::try_from(args.max_slices)
        .ok()
        .filter(|count| *count > 0)
    else {
        return usage_error("--max-slices must be greater than zero");
    };
    let payload = match plan_payload(&loaded, plan_selector(&args, repo_root), max_slices) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    if let Some(output_dir) = args.prompt_pack.as_deref() {
        ensure_prompt_pack_target(output_dir)?;
        write_prompt_pack(
            "plan",
            &payload,
            &loaded,
            output_dir,
            args.include_repository_context.then_some(repo_root),
            args.excerpt_bytes,
            args.force,
        )?;
    }
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_plan_text(&payload)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn run_check(repo_root: &Path, args: CheckArgs) -> Result<i32> {
    if args.details && !(1..=10_000).contains(&args.limit) {
        return usage_error("--limit must be between 1 and 10000");
    }
    let (loaded, _) = report_or_missing(repo_root, args.report.as_deref())?;
    let incomplete_records = loaded
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| record.get("analysis_status").and_then(Value::as_str) != Some("analyzed"))
        .count();
    if incomplete_records > 0 && !args.allow_incomplete_evidence {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "incomplete_evidence",
            format!("report contains {incomplete_records} incomplete selected inventory record(s); rerun analysis with complete inputs or pass --allow-incomplete-evidence"),
        )
        .at("/files")
        .with_details(json!({"incomplete_record_count": incomplete_records}))
        .into());
    }
    let loaded_config = loaded
        .get("config")
        .cloned()
        .unwrap_or_else(config::default_config);
    let context_band = args
        .fail_on_context_band
        .map(ContextBand::as_str)
        .or_else(|| {
            loaded_config
                .pointer("/check/fail_on_context_band")
                .and_then(Value::as_str)
        })
        .unwrap_or("critical");
    let slop_band = args
        .fail_on_slop_band
        .map(SlopBand::as_str)
        .or_else(|| {
            loaded_config
                .pointer("/check/fail_on_slop_band")
                .and_then(Value::as_str)
        })
        .unwrap_or("critical");
    let failures = failing_records_in(
        &loaded,
        Some(context_band),
        Some(slop_band),
        args.include_folders,
    );
    if !matches!(args.format, CheckFormat::Text) {
        match args.format {
            CheckFormat::Json => {
                let mut payload = json!({
                    "schema_version": 1,
                    "command": "check",
                    "report": {"schema_version": loaded.get("schema_version"), "analyzer": loaded.get("analyzer"), "repo": loaded.get("repo"), "scope": loaded.get("scope")},
                    "boundary": {"context_band": context_band, "slop_band": slop_band},
                    "passed": failures.is_empty(),
                    "finding_count": failures.len(),
                    "details_included": args.details,
                    "gate_scope": if args.include_folders { "files_and_folders" } else { "files" },
                });
                if args.details {
                    let findings = failures
                        .iter()
                        .skip(args.offset)
                        .take(args.limit)
                        .cloned()
                        .collect::<Vec<_>>();
                    payload["findings"] = json!(findings);
                    payload["collection"] = json!({
                        "total": failures.len(),
                        "offset": args.offset,
                        "limit": args.limit,
                        "returned": payload["findings"].as_array().map(Vec::len).unwrap_or_default(),
                        "truncated": args.offset.saturating_add(args.limit) < failures.len(),
                    });
                }
                print_text(&render_json(&payload)?);
            }
            CheckFormat::Github => {
                for failure in &failures {
                    let path = crate::text::github_property_escape(
                        failure
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    );
                    println!(
                        "::error file={}::Git Slop context={} slop={} score={}",
                        path,
                        failure
                            .get("context_band")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        failure
                            .get("slop_band")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        failure.get("slop_score").unwrap_or(&Value::Null)
                    );
                }
            }
            CheckFormat::Text => {}
        }
        return Ok(if failures.is_empty() { 0 } else { 1 });
    }
    if failures.is_empty() {
        println!(
            "Check passed: no file records met or exceeded context={context_band} or slop={slop_band}."
        );
        return Ok(0);
    }
    println!(
        "Check failed: {} file records met or exceeded context={context_band} or slop={slop_band}.",
        failures.len()
    );
    for failure in failures.iter().take(10) {
        println!(
            "- {} (slop={}, context={}, slop_score={})",
            safe_terminal(
                failure
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            failure
                .get("slop_band")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            failure
                .get("context_band")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            failure
                .get("slop_score")
                .map(ToString::to_string)
                .unwrap_or_else(|| "null".to_string()),
        );
    }
    Ok(1)
}

fn bounded_compare_output(
    payload: &Value,
    detail: CompareDetail,
    top: usize,
    offset: usize,
    limit: usize,
) -> Result<Value> {
    if limit == 0 {
        anyhow::bail!("--limit must be greater than zero");
    }
    let mut bounded = payload.clone();
    let cap = match detail {
        CompareDetail::Summary => 0,
        CompareDetail::Top => top,
        CompareDetail::Full => limit,
    };
    let start = if matches!(detail, CompareDetail::Full) {
        offset
    } else {
        0
    };
    let mut pagination = serde_json::Map::new();
    for key in [
        "file_deltas",
        "folder_deltas",
        "queue_movement",
        "overlay_deltas",
        "regressions",
    ] {
        let values = payload
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let returned = values
            .iter()
            .skip(start)
            .take(cap)
            .cloned()
            .collect::<Vec<_>>();
        pagination.insert(
            key.to_string(),
            json!({
                "total": values.len(),
                "offset": start,
                "limit": cap,
                "returned": returned.len(),
                "has_more": start.saturating_add(returned.len()) < values.len()
            }),
        );
        bounded[key] = json!(returned);
    }
    bounded["detail"] = json!(match detail {
        CompareDetail::Summary => "summary",
        CompareDetail::Top => "top",
        CompareDetail::Full => "full",
    });
    bounded["pagination"] = Value::Object(pagination);
    Ok(bounded)
}

fn render_compare_ndjson(payload: &Value) -> Result<String> {
    let mut lines = vec![render_json(&json!({
        "record_type": "summary",
        "schema_version": payload.get("schema_version"),
        "stream": {
            "schema": "schemas/compare-ndjson-1.json",
            "record_types": ["summary", "file_delta", "folder_delta", "queue_movement", "overlay_delta", "regression"]
        },
        "summary": payload.get("summary"),
        "pagination": payload.get("pagination"),
        "baseline_status": payload.get("baseline_status")
    }))?];
    for (key, record_type) in [
        ("file_deltas", "file_delta"),
        ("folder_deltas", "folder_delta"),
        ("queue_movement", "queue_movement"),
        ("overlay_deltas", "overlay_delta"),
        ("regressions", "regression"),
    ] {
        for record in payload
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            lines.push(render_json(
                &json!({"record_type": record_type, "record": record}),
            )?);
        }
    }
    Ok(lines.join("\n"))
}

fn baseline_path(repo_root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_baseline_name",
            "Baseline names must be 1-64 ASCII letters, digits, dots, dashes, or underscores.",
        )
        .at("/name")
        .with_details(json!({"name": name}))
        .into());
    }
    Ok(config::git_runtime_dir(repo_root)?
        .join("baselines")
        .join(format!("{name}.json")))
}

fn write_named_baseline(path: &Path, report: &Value, replace: bool) -> Result<()> {
    if path.exists() && !replace {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "baseline_exists",
            format!("Baseline already exists: {}", path.display()),
        )
        .at("/name")
        .with_details(json!({"path": path}))
        .into());
    }
    let parent = path.parent().expect("baseline path has parent");
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".baseline-{}-tmp", std::process::id()));
    fs::write(&temporary, render_json(report)?)?;
    let backup = parent.join(format!(".baseline-{}-backup", std::process::id()));
    let had_existing = path.exists();
    if had_existing {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        return Err(error.into());
    }
    if had_existing {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn run_baseline(repo_root: &Path, args: BaselineArgs) -> Result<i32> {
    match args.command {
        BaselineCommand::Create {
            name,
            report,
            force,
        } => {
            let (loaded, source) = report_or_missing(repo_root, report.as_deref())?;
            let path = baseline_path(repo_root, &name)?;
            write_named_baseline(&path, &loaded, force)?;
            println!(
                "Created baseline '{name}' from {} in Git-private runtime storage.",
                source.display()
            );
            Ok(0)
        }
        BaselineCommand::Update { name, report } => {
            let path = baseline_path(repo_root, &name)?;
            if !path.exists() {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            }
            let (loaded, source) = report_or_missing(repo_root, report.as_deref())?;
            write_named_baseline(&path, &loaded, true)?;
            println!("Updated baseline '{name}' from {}.", source.display());
            Ok(0)
        }
        BaselineCommand::Inspect { name, format } => {
            let path = baseline_path(repo_root, &name)?;
            let Some(report) = load_report_at(&path)? else {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            };
            let payload = json!({
                "schema_version": 1,
                "command": "baseline inspect",
                "name": name,
                "storage": "git_private",
                "report": {
                    "schema_version": report.get("schema_version"),
                    "generated_at": report.get("generated_at"),
                    "head_sha": report.pointer("/repo/head_sha"),
                    "scope": report.get("scope"),
                    "report_profile": report.pointer("/analyzer/report_profile"),
                    "evidence_completeness": report.get("evidence_completeness")
                }
            });
            match format {
                DisplayFormat::Json => print_text(&render_json(&payload)?),
                DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
                DisplayFormat::Text => println!(
                    "baseline={} revision={} generated_at={} storage=git_private",
                    payload["name"].as_str().unwrap_or_default(),
                    payload
                        .pointer("/report/head_sha")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown"),
                    payload
                        .pointer("/report/generated_at")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                ),
            }
            Ok(0)
        }
        BaselineCommand::Validate { name } => {
            let path = baseline_path(repo_root, &name)?;
            let Some(_) = load_report_at(&path)? else {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            };
            println!("Baseline '{name}' is valid.");
            Ok(0)
        }
        BaselineCommand::Remove { name } => {
            let path = baseline_path(repo_root, &name)?;
            if !path.exists() {
                return Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "baseline_not_found",
                    format!("Baseline not found: {name}"),
                )
                .at("/name")
                .with_details(json!({"name": name}))
                .into());
            }
            fs::remove_file(path)?;
            println!("Removed baseline '{name}'.");
            Ok(0)
        }
    }
}

fn run_compare(repo_root: &Path, args: CompareArgs) -> Result<i32> {
    let head_report = report_or_missing(Path::new(""), Some(&args.head))?.0;
    let inferred_scope = args.scope.clone().or_else(|| {
        head_report
            .pointer("/scope/path")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    });
    let analysis_clock = head_report
        .pointer("/analyzer/analysis_clock")
        .or_else(|| head_report.get("generated_at"))
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&chrono::Utc));
    let materialized = if let Some(reference) = args.base_ref.as_deref() {
        Some(crate::baseline::MaterializedBaseline::create(
            repo_root,
            reference,
            inferred_scope,
            args.allow_shallow,
            analysis_clock,
        )?)
    } else {
        None
    };
    let named_baseline_path = args
        .baseline
        .as_deref()
        .map(|name| baseline_path(repo_root, name))
        .transpose()?;
    let base_path = args
        .base
        .as_deref()
        .map(Path::to_path_buf)
        .or_else(|| materialized.as_ref().map(|value| value.report_path.clone()))
        .or(named_baseline_path)
        .expect("Clap requires --base, --base-ref, or --baseline");
    let base_report = report_or_missing(Path::new(""), Some(&base_path))?.0;
    let Some(top) = usize::try_from(args.top).ok().filter(|count| *count > 0) else {
        return usage_error("--top must be greater than zero.");
    };
    let base_descriptor = if let Some(materialized) = materialized.as_ref() {
        format!(
            "git:{}@{}",
            args.base_ref.as_deref().unwrap_or("unknown"),
            materialized.revision
        )
    } else if let Some(name) = args.baseline.as_deref() {
        format!("baseline:{name}")
    } else {
        base_path.to_string_lossy().into_owned()
    };
    let payload = match compare_payload_with_policy(
        &base_report,
        &head_report,
        Some(&base_descriptor),
        Some(&args.head.to_string_lossy()),
        top,
        args.force,
        args.allow_incomplete_evidence,
        args.policy_from.as_str(),
    ) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    let output = match bounded_compare_output(&payload, args.detail, top, args.offset, args.limit) {
        Ok(output) => output,
        Err(error) => return usage_error(error),
    };
    let mut output = output;
    if let Some(materialized) = materialized.as_ref() {
        output["baseline_materialization"] = json!({
            "reference": args.base_ref,
            "resolved_revision": materialized.revision,
            "isolated_worktree": true,
            "copied_head_config": materialized.copied_head_config,
            "cache_disabled": true
        });
    }
    match args.format {
        CompareFormat::Json => print_text(&render_json(&output)?),
        CompareFormat::Text => print_text(&render_compare_text(&output, top)),
        CompareFormat::Yaml => print_text(&serde_yaml::to_string(&output)?),
        CompareFormat::Ndjson => print_text(&render_compare_ndjson(&output)?),
    }
    let regressions = payload
        .pointer("/summary/regression_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    Ok(if args.fail_on_regression && regressions > 0 {
        1
    } else {
        0
    })
}

fn run_report(args: ReportArgs) -> Result<i32> {
    match args.command {
        ReportCommand::Validate { path, allow_legacy } => {
            match report::load_report_with_legacy(&path, allow_legacy) {
                Ok(value) => {
                    println!(
                        "Report is valid: {} (schema {}).",
                        path.display(),
                        value["schema_version"]
                    );
                    Ok(0)
                }
                Err(error) => Err(ClassifiedError::new(
                    ErrorKind::Contract,
                    "report_invalid",
                    format!("{error:#}"),
                )
                .at("/report")
                .with_details(json!({"path": path}))
                .into()),
            }
        }
        ReportCommand::Migrate { path, output } => {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let value: Value = serde_json::from_str(&source)
                .with_context(|| format!("invalid git-slop report JSON: {}", path.display()))?;
            let migrated = report::migrate_legacy_report(value)?;
            report::write_json_atomically(&output, &migrated)?;
            println!(
                "Migrated {} to schema 5 at {}.",
                path.display(),
                output.display()
            );
            Ok(0)
        }
        ReportCommand::Schema => {
            print_text(&render_json(&report::schema())?);
            Ok(0)
        }
    }
}

fn run_sarif(repo_root: &Path, args: SarifArgs) -> Result<i32> {
    let (loaded, report_path) = report_or_missing(repo_root, args.report.as_deref())?;
    let top = match args.top {
        None => None,
        Some(value) => match usize::try_from(value).ok().filter(|count| *count > 0) {
            Some(value) => Some(value),
            None => return usage_error("--top must be greater than zero."),
        },
    };
    let payload = match sarif_payload(&loaded, Some(&report_path.to_string_lossy()), top) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    let rendered = render_json(&payload)?;
    if let Some(output) = args.output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&output, rendered)
            .with_context(|| format!("failed to write {}", output.display()))?;
        println!("Wrote SARIF report to {}.", output.display());
    } else {
        print_text(&rendered);
    }
    Ok(0)
}

fn run_health(repo_root: &Path, args: HealthArgs) -> Result<i32> {
    let (mut loaded, _) = report_or_missing(repo_root, args.report.as_deref())?;
    let rollup = match health::health_rollup_from_report(&loaded) {
        Ok(rollup) => rollup,
        Err(error) => return usage_error(error),
    };
    let mut health_value = serde_json::to_value(rollup)?;
    if let (Some(existing), Some(derived)) = (
        loaded.get("health").and_then(Value::as_object),
        health_value.as_object_mut(),
    ) {
        for (key, value) in existing {
            derived.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    if let Some(object) = loaded.as_object_mut() {
        object.insert("health".to_string(), health_value);
    }
    match args.format {
        HealthFormat::Text => print_text(&report::render_terminal(&loaded)),
        HealthFormat::Markdown => {
            let rendered = match health::render_health_from_report(&loaded) {
                Ok(rendered) => rendered,
                Err(error) => return usage_error(error),
            };
            print_text(&rendered);
        }
        HealthFormat::Github => {
            print_text(&render_github_annotations(&loaded, args.max_annotations));
        }
        HealthFormat::Json => {
            print_text(&render_json(&health_json_payload(&loaded))?);
        }
    }
    Ok(0)
}

fn diff_values(current: &Value, defaults: &Value) -> Value {
    match (current, defaults) {
        (Value::Object(current), Value::Object(defaults)) => {
            let mut result = serde_json::Map::new();
            for (key, value) in current {
                if matches!(key.as_str(), "tokenizer" | "context_bands") {
                    continue;
                }
                let difference = defaults
                    .get(key)
                    .map_or_else(|| value.clone(), |default| diff_values(value, default));
                if !difference.is_null()
                    && !difference
                        .as_object()
                        .is_some_and(serde_json::Map::is_empty)
                {
                    result.insert(key.clone(), difference);
                }
            }
            Value::Object(result)
        }
        _ if current == defaults => Value::Null,
        _ => current.clone(),
    }
}

fn load_config_contract(repo_root: &Path) -> Result<Value> {
    config::load(repo_root).map_err(|error| {
        ClassifiedError::new(
            ErrorKind::Contract,
            "invalid_configuration",
            format!("{error:#}"),
        )
        .at("/.slop/config.yaml")
        .into()
    })
}

fn run_config(repo_root: &Path, args: ConfigArgs) -> Result<i32> {
    match args.command {
        ConfigCommand::Show { effective } => {
            if effective {
                print_text(&serde_yaml::to_string(&load_config_contract(repo_root)?)?);
            } else {
                let path = config::config_path(repo_root);
                if path.exists() {
                    print_text(&fs::read_to_string(path)?);
                } else {
                    print_text(config::MINIMAL_CONFIG);
                }
            }
        }
        ConfigCommand::Validate => {
            load_config_contract(repo_root)?;
            let path = config::config_path(repo_root);
            if path.exists() {
                println!("Configuration is valid: {}", path.display());
            } else {
                println!(
                    "Configuration is valid: built-in defaults ({} is absent).",
                    path.display()
                );
            }
        }
        ConfigCommand::DiffDefaults => {
            let diff = diff_values(&load_config_contract(repo_root)?, &config::default_config());
            print_text(&serde_yaml::to_string(&diff)?);
        }
        ConfigCommand::Migrate => {
            let effective = load_config_contract(repo_root)?;
            let mut diff = diff_values(&effective, &config::default_config());
            if let Some(object) = diff.as_object_mut() {
                object.insert("schema_version".into(), json!(2));
            }
            config::ensure_state_dirs(repo_root)?;
            fs::write(
                config::config_path(repo_root),
                serde_yaml::to_string(&diff)?,
            )?;
            println!(
                "Migrated {} to schema 2.",
                config::config_path(repo_root).display()
            );
        }
        ConfigCommand::Schema => print_text(&render_json(&config::schema())?),
    }
    Ok(0)
}

fn run_doctor(repo_root: &Path, args: DoctorArgs) -> Result<i32> {
    let repo = git::repo_metadata(repo_root)?;
    let config_result = config::load(repo_root);
    let config_exists = config::config_path(repo_root).is_file();
    let report_path = default_report_path(repo_root);
    let report_status = if report_path.exists() {
        match report::load_report(&report_path) {
            Ok(_) => "compatible",
            Err(_) => "invalid",
        }
    } else {
        "missing"
    };
    let normalized_scope = analyze::normalize_scope(args.scope.as_deref()).map_err(|error| {
        ClassifiedError::new(ErrorKind::Contract, "invalid_scope", format!("{error:#}"))
            .at("/scope")
            .with_details(json!({"scope": args.scope}))
    })?;
    if let Some(scope) = normalized_scope.as_deref()
        && fs::symlink_metadata(repo_root.join(scope)).is_err()
    {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "scope_not_found",
            format!("--scope does not exist in the repository: {scope}"),
        )
        .at("/scope")
        .with_details(json!({"scope": scope}))
        .into());
    }
    let tracked_paths = git::list_tracked_files(repo_root)?
        .into_iter()
        .filter(|path| {
            normalized_scope
                .as_deref()
                .is_none_or(|scope| path == scope || path.starts_with(&format!("{scope}/")))
        })
        .collect::<Vec<_>>();
    if tracked_paths.is_empty() {
        return Err(ClassifiedError::new(
            ErrorKind::Contract,
            "empty_scope",
            normalized_scope.as_deref().map_or_else(
                || "repository selected no tracked paths".to_string(),
                |scope| format!("--scope {scope:?} selected no tracked paths"),
            ),
        )
        .at("/scope")
        .with_details(json!({"scope": normalized_scope}))
        .into());
    }
    let tracked = tracked_paths.len();
    let effective_config = config_result
        .as_ref()
        .cloned()
        .unwrap_or_else(|_| config::default_config());
    let estimate = crate::estimate::build(repo_root, &tracked_paths, &effective_config);
    let resource_status = if estimate.estimated_peak_memory_bytes > estimate.memory_budget_bytes {
        "over_memory_budget"
    } else {
        "within_budget"
    };
    let bundle_path = args.bundle.as_ref().map(|output| {
        if output.is_absolute() {
            output.clone()
        } else {
            repo_root.join(output)
        }
    });
    let mut diagnostics = Vec::new();
    if let Err(error) = &config_result {
        diagnostics.push(json!({"code":"invalid_configuration","severity":"error","detail": crate::text::visible_controls(&format!("{error:#}"))}));
    }
    if report_status == "invalid" {
        diagnostics.push(json!({"code":"invalid_latest_report","severity":"error","detail":"The latest report does not satisfy the supported report contract."}));
    } else if report_status == "missing" {
        diagnostics.push(json!({"code":"latest_report_missing","severity":"notice","detail":"No latest report exists yet."}));
    }
    if repo.is_shallow {
        diagnostics.push(json!({"code":"shallow_history","severity":"warning","detail":"Git history evidence is incomplete."}));
    }
    if resource_status == "over_memory_budget" {
        diagnostics.push(json!({"code":"estimated_memory_budget_exceeded","severity":"error","detail":format!("Estimated peak memory is {} bytes for a {} byte budget.", estimate.estimated_peak_memory_bytes, estimate.memory_budget_bytes)}));
    }
    let diagnostic = json!({
        "schema_version": 1,
        "command": "doctor",
        "status": if config_result.is_err() || report_status == "invalid" || resource_status == "over_memory_budget" { "error" } else { "ready" },
        "repository": {"name": repo.repo_name, "branch": repo.branch, "shallow": repo.is_shallow, "detached": repo.detached_head, "clean": repo.worktree_clean},
        "config": {"status": if config_result.is_err() { "invalid" } else if config_exists { "valid" } else { "using_defaults" }, "path": config::config_path(repo_root)},
        "report": {"status": report_status, "path": report_path},
        "estimate": estimate,
        "resource_status": resource_status,
        "diagnostics": diagnostics,
        "bundle_path": bundle_path,
        "recovery": {
            "config": "Run git slop config validate, then correct the reported key or run git slop config migrate.",
            "report": "Run git slop find to replace a missing or incompatible latest report.",
            "shallow": "Fetch full history or rerun find with --allow-shallow to acknowledge incomplete evidence."
        }
    });
    if matches!(args.format, DoctorFormat::Json) {
        print_text(&render_json(&diagnostic)?);
    } else {
        println!("Git Slop doctor");
        println!("- git: available");
        println!("- repository: {}", repo.repo_name);
        println!(
            "- branch: {}",
            repo.branch.as_deref().unwrap_or(if repo.detached_head {
                "detached HEAD"
            } else {
                "unborn"
            })
        );
        println!(
            "- history: {}",
            if repo.is_shallow {
                "shallow (incomplete)"
            } else {
                "complete"
            }
        );
        println!(
            "- worktree: {} (staged={}, modified={}, untracked={})",
            if repo.worktree_clean {
                "clean"
            } else {
                "dirty"
            },
            repo.staged_change_count,
            repo.modified_tracked_file_count,
            repo.untracked_file_count
        );
        println!(
            "- config: {}",
            if config_result.is_ok() {
                if config_exists {
                    "valid"
                } else {
                    "using built-in defaults"
                }
            } else {
                "invalid"
            }
        );
        println!("- report: {report_status}");
        println!(
            "- preflight: {tracked} tracked files; peak memory ~{} MiB; cache ~{} MiB; report ~{} MiB; time ~{}s; inodes ~{}",
            estimate.estimated_peak_memory_bytes.div_ceil(1024 * 1024),
            estimate.estimated_cache_bytes.div_ceil(1024 * 1024),
            estimate.estimated_report_bytes.div_ceil(1024 * 1024),
            estimate.estimated_seconds,
            estimate.estimated_inode_count,
        );
        if repo.is_shallow {
            println!("- recovery: fetch full history, or explicitly use --allow-shallow");
        }
        if config_result.is_err() {
            println!("- recovery: run `git slop config validate` and correct the reported key");
        }
        if report_status != "compatible" {
            println!("- recovery: run `git slop find` to produce a compatible latest report");
        }
    }
    if let Some(output) = bundle_path {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let config_digest = config_result
            .as_ref()
            .ok()
            .and_then(|value| serde_json::to_vec(value).ok())
            .map(|bytes| hex::encode(sha2::Sha256::digest(bytes)));
        let payload = json!({
            "schema_version": 1,
            "git_slop_version": VERSION,
            "repository": {"name": repo.repo_name, "shallow": repo.is_shallow, "detached": repo.detached_head, "clean": repo.worktree_clean, "staged": repo.staged_change_count, "modified": repo.modified_tracked_file_count, "untracked_count": repo.untracked_file_count},
            "config_digest": config_digest,
            "report_status": report_status,
            "diagnostics": diagnostics,
            "estimate": estimate,
            "privacy": {"source_included": false, "raw_tokens_included": false, "absolute_paths_included": false, "author_identities_included": false, "credentials_included": false}
        });
        fs::write(&output, render_json(&payload)?)?;
        if matches!(args.format, DoctorFormat::Json) {
            eprintln!("Wrote redacted diagnostic bundle to {}.", output.display());
        } else {
            println!("Wrote redacted diagnostic bundle to {}.", output.display());
        }
    }
    Ok(
        if config_result.is_err()
            || report_status == "invalid"
            || resource_status == "over_memory_budget"
        {
            2
        } else {
            0
        },
    )
}

fn matches_list_filter(
    item: &Value,
    args: &ListFilterArgs,
    kind: &str,
    files: &std::collections::BTreeMap<String, Value>,
) -> bool {
    let candidate_paths = match kind {
        "relationships" => ["source_path", "target_path"]
            .into_iter()
            .filter_map(|key| item.get(key).and_then(Value::as_str))
            .collect::<Vec<_>>(),
        "clusters" => item
            .get("member_paths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect(),
        _ => item
            .get("path")
            .and_then(Value::as_str)
            .into_iter()
            .collect(),
    };
    let field_matches = |field: &str, expected: &str| {
        item.get(field).and_then(Value::as_str) == Some(expected)
            || candidate_paths.iter().any(|path| {
                files
                    .get(*path)
                    .and_then(|file| file.get(field))
                    .and_then(Value::as_str)
                    == Some(expected)
            })
    };
    args.path
        .as_ref()
        .is_none_or(|path| candidate_paths.iter().any(|value| value.starts_with(path)))
        && args.profile.as_ref().is_none_or(|value| {
            field_matches("profile", value)
                || (kind == "profiles" && item.get("name").and_then(Value::as_str) == Some(value))
        })
        && args
            .language
            .as_ref()
            .is_none_or(|value| field_matches("language", value))
        && args.classification.as_ref().is_none_or(|value| {
            field_matches("classification", value) || field_matches("class", value)
        })
        && args
            .severity
            .as_ref()
            .is_none_or(|value| item.get("severity").and_then(Value::as_str) == Some(value))
}

fn terminal_field(value: &str, width: usize, no_truncate: bool) -> String {
    let value = safe_terminal(value).replace(['\n', '\t'], " ");
    if no_truncate || value.chars().count() <= width {
        return value;
    }
    if width <= 1 {
        return "…".to_string();
    }
    value.chars().take(width - 1).collect::<String>() + "…"
}

fn run_list(repo_root: &Path, args: ListArgs) -> Result<i32> {
    let filter = match &args.command {
        ListCommand::Findings(v)
        | ListCommand::Relationships(v)
        | ListCommand::Clusters(v)
        | ListCommand::Profiles(v) => v,
    };
    let (loaded, _) = report_or_missing(repo_root, filter.report.as_deref())?;
    let kind = match &args.command {
        ListCommand::Findings(_) => "findings",
        ListCommand::Relationships(_) => "relationships",
        ListCommand::Clusters(_) => "clusters",
        ListCommand::Profiles(_) => "profiles",
    };
    let files = loaded
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| Some((record.get("path")?.as_str()?.to_string(), record.clone())))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut values = match &args.command {
        ListCommand::Findings(_) => loaded
            .pointer("/health/findings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        ListCommand::Relationships(_) => loaded
            .pointer("/overlays/organization_health/relationships")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.values())
            .filter_map(Value::as_array)
            .flatten()
            .cloned()
            .collect(),
        ListCommand::Clusters(_) => loaded
            .pointer("/overlays/organization_health/clusters")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.values())
            .filter_map(Value::as_array)
            .flatten()
            .cloned()
            .collect(),
        ListCommand::Profiles(_) => loaded
            .pointer("/health/profile_rollups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    let unfiltered_total = values.len();
    values.retain(|item| matches_list_filter(item, filter, kind, &files));
    let matched_total = values.len();
    values.truncate(filter.top);
    let returned = values.len();
    match filter.format {
        DisplayFormat::Json => print_text(&render_json(&json!({
            "schema_version": 1,
            "command": "list",
            "kind": kind,
            "items": values,
            "collection": {"total": unfiltered_total, "matched": matched_total, "returned": returned, "limit": filter.top, "truncated": returned < matched_total}
        }))?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&values)?),
        DisplayFormat::Text => {
            let terminal_width = std::env::var("COLUMNS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(100);
            let path_width = if filter.no_truncate {
                values
                    .iter()
                    .filter_map(|item| {
                        item.get("path")
                            .or_else(|| item.get("name"))
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str)
                    })
                    .map(str::len)
                    .max()
                    .unwrap_or(24)
                    .max(24)
            } else if filter.wide {
                80
            } else {
                terminal_width.saturating_sub(59).clamp(24, 48)
            };
            println!(
                "{:<path_width$}  {:<16}  {:<10}  {:<10}  {:<10}  {:>7}",
                "PATH OR NAME", "PROFILE", "SEVERITY", "CONTEXT", "SLOP", "SCORE"
            );
            println!(
                "{:-<path_width$}  {:-<16}  {:-<10}  {:-<10}  {:-<10}  {:-<7}",
                "", "", "", "", "", ""
            );
            for item in &values {
                let label = item
                    .get("path")
                    .or_else(|| item.get("name"))
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                let profile = item
                    .get("profile")
                    .or_else(|| item.get("kind"))
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                let severity = item.get("severity").and_then(Value::as_str).unwrap_or("-");
                let context = item
                    .get("context_band")
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                let slop = item.get("slop_band").and_then(Value::as_str).unwrap_or("-");
                let score = item
                    .get("slop_score")
                    .or_else(|| item.get("evidence_score"))
                    .and_then(Value::as_f64)
                    .map_or_else(|| "-".to_string(), |value| format!("{value:.3}"));
                println!(
                    "{:<path_width$}  {:<16}  {:<10}  {:<10}  {:<10}  {:>7}",
                    terminal_field(label, path_width, filter.no_truncate),
                    terminal_field(profile, 16, filter.no_truncate),
                    terminal_field(severity, 10, filter.no_truncate),
                    terminal_field(context, 10, filter.no_truncate),
                    terminal_field(slop, 10, filter.no_truncate),
                    score
                );
            }
            println!(
                "\nReturned {returned} of {matched_total} matching record(s) from {unfiltered_total} total.{}",
                if returned < matched_total {
                    " Increase --top to see more."
                } else {
                    ""
                }
            );
        }
    }
    Ok(0)
}

fn run_prune(repo_root: &Path, args: PruneArgs) -> Result<i32> {
    let loaded = config::load(repo_root).unwrap_or_else(|_| config::default_config());
    let keep = args
        .keep
        .unwrap_or_else(|| config::pointer_u64(&loaded, "/output/retention_runs", 20) as usize);
    let max_bytes = args
        .max_bytes
        .unwrap_or_else(|| config::pointer_u64(&loaded, "/output/retention_bytes", 2_147_483_648));
    let root = config::runs_dir(repo_root);
    if !root.exists() {
        let payload = json!({
            "schema_version": 1,
            "command": "prune",
            "dry_run": args.dry_run,
            "limits": {"max_runs": keep, "max_bytes": max_bytes},
            "before": {"runs": 0, "bytes": 0},
            "selected": [],
            "after": {"runs": 0, "bytes": 0}
        });
        match args.format {
            DisplayFormat::Text => println!("No run snapshots to prune."),
            DisplayFormat::Json => print_text(&render_json(&payload)?),
            DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
        }
        return Ok(0);
    }
    let mut runs = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| {
            let bytes = directory_size(&entry.path())?;
            Ok((entry, bytes))
        })
        .collect::<Result<Vec<_>>>()?;
    runs.sort_by_key(|(entry, _)| std::cmp::Reverse(entry.file_name()));
    let before_bytes = runs.iter().map(|(_, bytes)| *bytes).sum::<u64>();
    let before_runs = runs.len();
    let mut retained_bytes = 0u64;
    let mut retained_runs = 0usize;
    let mut remove = Vec::new();
    let mut retention_prefix_exhausted = false;
    for (entry, bytes) in runs {
        if !retention_prefix_exhausted
            && retained_runs < keep
            && retained_bytes.saturating_add(bytes) <= max_bytes
        {
            retained_runs += 1;
            retained_bytes = retained_bytes.saturating_add(bytes);
        } else {
            retention_prefix_exhausted = true;
            remove.push((entry, bytes));
        }
    }
    let selected = remove
        .iter()
        .map(|(entry, bytes)| json!({"path": entry.path(), "bytes": bytes}))
        .collect::<Vec<_>>();
    if args.format == DisplayFormat::Text {
        for (entry, _) in &remove {
            println!(
                "{} {}",
                if args.dry_run {
                    "Would remove"
                } else {
                    "Removing"
                },
                entry.path().display()
            );
        }
    }
    for (entry, _) in &remove {
        if !args.dry_run {
            fs::remove_dir_all(entry.path())?;
        }
    }
    let removed_bytes = remove.iter().map(|(_, bytes)| *bytes).sum::<u64>();
    let payload = json!({
        "schema_version": 1,
        "command": "prune",
        "dry_run": args.dry_run,
        "limits": {"max_runs": keep, "max_bytes": max_bytes},
        "before": {"runs": before_runs, "bytes": before_bytes},
        "selected": selected,
        "removed": {"runs": remove.len(), "bytes": removed_bytes},
        "after": {"runs": retained_runs, "bytes": retained_bytes, "projected": args.dry_run}
    });
    match args.format {
        DisplayFormat::Text => println!(
            "{} {} old run snapshot(s) ({} bytes); retained {} run(s) ({} bytes).",
            if args.dry_run { "Selected" } else { "Pruned" },
            remove.len(),
            removed_bytes,
            retained_runs,
            retained_bytes
        ),
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            total = total.saturating_add(directory_size(&entry.path())?);
        } else {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

fn run_cache(repo_root: &Path, args: CacheArgs) -> Result<i32> {
    let state_root = args.state_dir.map_or_else(
        || config::slop_dir(repo_root),
        |path| {
            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        },
    );
    let (payload, format) = match args.command {
        CacheCommand::Status { format } => (crate::cache::status(&state_root)?, format),
        CacheCommand::Prune {
            max_entries,
            max_bytes,
            dry_run,
            compact,
            format,
        } => (
            crate::cache::prune(&state_root, max_entries, max_bytes, dry_run, compact)?,
            format,
        ),
    };
    match format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
        DisplayFormat::Text => {
            if payload["command"] == "cache status" {
                println!(
                    "Cache {}: {} entries, {} payload bytes, {} database bytes.",
                    payload["status"].as_str().unwrap_or("unknown"),
                    payload["entries"],
                    payload["payload_bytes"],
                    payload["database_bytes"]
                );
            } else {
                println!(
                    "{} {} cache entries ({} payload bytes).",
                    if payload["dry_run"].as_bool().unwrap_or(false) {
                        "Would prune"
                    } else {
                        "Pruned"
                    },
                    payload["removed_entries"],
                    payload["removed_payload_bytes"]
                );
            }
        }
    }
    Ok(0)
}

fn run_completions(args: CompletionsArgs) -> Result<i32> {
    let mut command = Cli::command();
    let mut stdout = std::io::stdout().lock();
    match args.shell {
        CompletionShell::Bash => generate(Shell::Bash, &mut command, PROJECT_NAME, &mut stdout),
        CompletionShell::Zsh => generate(Shell::Zsh, &mut command, PROJECT_NAME, &mut stdout),
        CompletionShell::Fish => generate(Shell::Fish, &mut command, PROJECT_NAME, &mut stdout),
        CompletionShell::Powershell => {
            generate(Shell::PowerShell, &mut command, PROJECT_NAME, &mut stdout)
        }
        CompletionShell::Nushell => generate(
            clap_complete_nushell::Nushell,
            &mut command,
            PROJECT_NAME,
            &mut stdout,
        ),
    }
    Ok(0)
}

fn write_generated_output(output: Option<&Path>, bytes: &[u8]) -> Result<()> {
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    } else {
        use std::io::Write;
        std::io::stdout().lock().write_all(bytes)?;
    }
    Ok(())
}

fn run_man(args: ManArgs) -> Result<i32> {
    let mut bytes = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut bytes)?;
    let rendered = String::from_utf8(bytes).context("generated manual was not UTF-8")?;
    let normalized = format!(
        "{}\n",
        rendered
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    );
    write_generated_output(args.output.as_deref(), normalized.as_bytes())?;
    Ok(0)
}

fn markdown_command(command: &clap::Command, path: &str, output: &mut String) {
    output.push_str(&format!("## `{path}`\n\n"));
    if let Some(about) = command.get_about() {
        output.push_str(&format!("{about}\n\n"));
    }
    let arguments = command.get_arguments().collect::<Vec<_>>();
    if !arguments.is_empty() {
        output.push_str("| Argument | Description |\n| --- | --- |\n");
        for argument in arguments {
            let name = argument
                .get_long()
                .map(|value| format!("--{value}"))
                .unwrap_or_else(|| argument.get_id().to_string());
            let help = argument
                .get_help()
                .map(ToString::to_string)
                .unwrap_or_default();
            output.push_str(&format!("| `{name}` | {} |\n", help.replace('|', "\\|")));
        }
        output.push('\n');
    }
    for subcommand in command.get_subcommands() {
        markdown_command(
            subcommand,
            &format!("{path} {}", subcommand.get_name()),
            output,
        );
    }
}

fn run_reference(args: ReferenceArgs) -> Result<i32> {
    let command = Cli::command();
    let mut markdown =
        "# Git Slop CLI Reference\n\nGenerated from the live Clap command tree.\n\n".to_string();
    markdown_command(&command, "git-slop", &mut markdown);
    let normalized = format!("{}\n", markdown.trim_end());
    write_generated_output(args.output.as_deref(), normalized.as_bytes())?;
    Ok(0)
}

fn run_html(repo_root: &Path, args: HtmlArgs) -> Result<i32> {
    let explicit_report = args.report.clone();
    let (loaded, report_path) = report_or_missing(repo_root, args.report.as_deref())?;
    let output = args.output.unwrap_or_else(|| {
        explicit_report
            .as_ref()
            .and_then(|path| path.parent())
            .map_or_else(|| config::latest_dir(repo_root), Path::to_path_buf)
            .join("report.html")
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let bounded = |pointer: &str, limit: usize| {
        loaded
            .pointer(pointer)
            .and_then(Value::as_array)
            .map(|records| records.iter().take(limit).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let bounded_sections = |pointer: &str, limit: usize| {
        loaded
            .pointer(pointer)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|sections| sections.values())
            .filter_map(Value::as_array)
            .flatten()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
    };
    let embedded_limit = 5_000usize;
    let source_report = relative_display(&report_path, repo_root);
    let payload = serde_json::to_string(&json!({
        "schema_version": loaded.get("schema_version"),
        "generated_at": loaded.get("generated_at"),
        "analyzed_revision_at": loaded.get("analyzed_revision_at"),
        "analyzer": loaded.get("analyzer"),
        "repo": loaded.get("repo"),
        "scope": loaded.get("scope"),
        "config_digests": loaded.pointer("/analyzer/config_digests"),
        "collection_metadata": loaded.get("collection_metadata"),
        "evidence_completeness": loaded.get("evidence_completeness"),
        "files": bounded("/files", embedded_limit),
        "folders": bounded("/folders", embedded_limit),
        "action_queue": bounded("/action_queue", embedded_limit),
        "health": {
            "summary": loaded.pointer("/health/summary"),
            "findings": bounded("/health/findings", embedded_limit)
        },
        "organization": {
            "relationships": bounded_sections("/overlays/organization_health/relationships", embedded_limit),
            "clusters": bounded_sections("/overlays/organization_health/clusters", embedded_limit)
        },
        "embedded_evidence": {"record_limit_per_view": embedded_limit},
        "source_report": source_report
    }))?
    .replace("</", "<\\/");
    let csp_nonce = &hex::encode(sha2::Sha256::digest(payload.as_bytes()))[..24];
    let html = format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'nonce-{csp_nonce}'; script-src 'nonce-{csp_nonce}'; base-uri 'none'; form-action 'none'"><title>Git Slop local report</title><style nonce="{csp_nonce}">
:root {{ color-scheme: light dark; font: 15px system-ui,sans-serif }} body {{ margin: 2rem; max-width: 1100px }}
input,select {{ padding:.55rem; margin:0 .5rem .75rem 0 }} table {{ width:100%; border-collapse:collapse }}
th,td {{ text-align:left; padding:.5rem; border-bottom:1px solid #8885 }} th button {{ all:unset;cursor:pointer;font-weight:700 }}
code {{ overflow-wrap:anywhere }} details {{ margin:1rem 0 }} .muted {{ opacity:.7 }} .sr {{ position:absolute;left:-10000px }}
.views button[aria-pressed="true"] {{ font-weight:700;text-decoration:underline }} tr:target {{ outline:2px solid currentColor }}
</style></head><body><h1>Git Slop local report</h1><p id="descriptor" class="muted"></p>
<nav class="views" aria-label="Report view"><button data-view="files" aria-pressed="true">Files</button> <button data-view="folders" aria-pressed="false">Folders</button> <button data-view="queue" aria-pressed="false">Action queue</button> <button data-view="health" aria-pressed="false">Health findings</button> <button data-view="relationships" aria-pressed="false">Relationships</button> <button data-view="clusters" aria-pressed="false">Clusters</button></nav>
<label for="query" class="sr">Search paths</label><input id="query" type="search" placeholder="Search paths"><label for="profile" class="sr">Profile</label><select id="profile"><option value="">All profiles</option></select>
<label id="severity-label" for="severity" class="sr">Maintenance band</label><select id="severity"><option value="">All maintenance bands</option><option>critical</option><option>high</option><option>moderate</option><option>low</option><option>error</option><option>warning</option><option>notice</option></select>
<p id="sort-state" class="muted" aria-live="polite"></p><p id="count" aria-live="polite"></p><button id="previous" type="button">Previous</button><button id="next" type="button">Next</button><table><caption class="sr">Git Slop records</caption><thead><tr id="headers"></tr></thead><tbody id="rows"></tbody></table>
<details id="file-detail"><summary>Selected record details</summary><pre id="detail"></pre></details>
<details><summary>Evidence summary</summary><pre id="evidence-summary"></pre></details>
<script id="report" type="application/json">{payload}</script><script nonce="{csp_nonce}">
const report=JSON.parse(document.getElementById('report').textContent), params=new URLSearchParams(location.search); let view=params.get('view')||'files', sortKey=params.get('sort')||'slop_score', ascending=params.get('dir')==='asc', page=Number(params.get('page')||0); const pageSize=100;
const files=report.files??[], folders=report.folders??[], queue=report.action_queue??[], findings=report.health?.findings??[], relationships=report.organization?.relationships??[], clusters=report.organization?.clusters??[]; const esc=v=>String(v??'').replace(/[&<>"']/g,c=>({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c]));
document.getElementById('descriptor').textContent=`${{report.repo?.repo_name||'repository'}} · ${{report.generated_at||'unknown time'}} · schema ${{report.schema_version}}`;
const profile=document.getElementById('profile'); [...new Set(files.map(f=>f.profile).filter(Boolean))].sort().forEach(v=>profile.insertAdjacentHTML('beforeend',`<option>${{esc(v)}}</option>`));
document.getElementById('query').value=params.get('q')||''; profile.value=params.get('profile')||''; document.getElementById('severity').value=params.get('band')||'';
const columns={{files:[['path','Path'],['profile','Profile'],['language','Language'],['slop_band','Maintenance'],['context_band','Context'],['slop_score','Score'],['tokens','Tokens']],folders:[['path','Folder'],['classification','Classification'],['health_band','Health'],['context_band','Context'],['slop_score','Score'],['tokens','Tokens']],queue:[['path','Path'],['severity','Severity'],['reason_code','Reason'],['evidence_status','Evidence'],['next_action','Next action']],health:[['path','Path'],['severity','Severity'],['title','Finding'],['message','Message']],relationships:[['id','Relationship'],['kind','Kind'],['source_path','Source'],['target_path','Target'],['evidence_score','Evidence']],clusters:[['id','Cluster'],['kind','Kind'],['member_count','Members'],['evidence_score','Evidence']]}};
function records() {{ return view==='folders'?folders:view==='queue'?queue:view==='health'?findings:view==='relationships'?relationships:view==='clusters'?clusters:files }}
function syncUrl() {{ const p=new URLSearchParams(); for (const [k,v] of Object.entries({{view,q:document.getElementById('query').value,profile:profile.value,band:document.getElementById('severity').value,sort:sortKey,dir:ascending?'asc':'desc',page}})) if(v!==''&&v!==0)p.set(k,v); history.replaceState(null,'',`${{location.pathname}}?${{p}}${{location.hash}}`) }}
function render() {{ const q=document.getElementById('query').value.toLowerCase(), p=profile.value, s=document.getElementById('severity').value, source=records();
 document.querySelectorAll('[data-view]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.view===view)));
 const activeColumns=columns[view]??columns.files; if(!activeColumns.some(([key])=>key===sortKey))sortKey=activeColumns[0][0]; document.getElementById('headers').innerHTML=activeColumns.map(([key,label])=>`<th scope="col"><button data-key="${{esc(key)}}" aria-sort="${{key===sortKey?(ascending?'ascending':'descending'):'none'}}">${{esc(label)}}</button></th>`).join('');
 document.getElementById('severity-label').textContent=view==='health'||view==='queue'?'Finding severity':'Maintenance band';
 const haystack=f=>[f.path,f.id,f.source_path,f.target_path,...(f.members??[])].join(' ').toLowerCase(); const selected=source.filter(f=>(!q||haystack(f).includes(q))&&(!p||f.profile===p)&&(!s||(f.slop_band??f.severity)===s)).sort((a,b)=>{{const x=a[sortKey],y=b[sortKey]; return (typeof x==='number'?x-y:String(x??'').localeCompare(String(y??'')))*(ascending?1:-1)}});
 const pages=Math.max(1,Math.ceil(selected.length/pageSize)); page=Math.min(page,pages-1); const visible=selected.slice(page*pageSize,(page+1)*pageSize);
 document.getElementById('count').textContent=`${{selected.length}} of ${{source.length}} ${{view.replace('_',' ')}} records · page ${{page+1}} of ${{pages}}`;
 document.getElementById('previous').disabled=page===0; document.getElementById('next').disabled=page+1>=pages;
 document.getElementById('sort-state').textContent=`Sorted by ${{activeColumns.find(([key])=>key===sortKey)?.[1]??sortKey}}, ${{ascending?'ascending':'descending'}}`;
 document.getElementById('rows').innerHTML=visible.map((f,i)=>`<tr tabindex="0" id="record-${{page*pageSize+i}}" data-index="${{page*pageSize+i}}">${{activeColumns.map(([key],column)=>`<td>${{column===0?`<button class="record"><code>${{esc(f[key]??f.path??f.id)}}</code></button>`:esc(f[key]??(key==='member_count'?(f.members??[]).length:''))}}</td>`).join('')}}</tr>`).join('');
 document.querySelectorAll('.record').forEach((button,i)=>button.addEventListener('click',()=>{{document.getElementById('detail').textContent=JSON.stringify(visible[i],null,2);document.getElementById('file-detail').open=true;location.hash=`record-${{page*pageSize+i}}`}})); syncUrl(); }}
document.querySelectorAll('input,select').forEach(el=>el.addEventListener('input',()=>{{page=0;render()}})); document.getElementById('headers').addEventListener('click',event=>{{const el=event.target.closest('button');if(!el)return;ascending=sortKey===el.dataset.key?!ascending:true;sortKey=el.dataset.key;page=0;render()}});
document.querySelectorAll('[data-view]').forEach(el=>el.addEventListener('click',()=>{{view=el.dataset.view;page=0;render()}})); document.getElementById('previous').addEventListener('click',()=>{{page=Math.max(0,page-1);render()}}); document.getElementById('next').addEventListener('click',()=>{{page+=1;render()}});
document.addEventListener('keydown',event=>{{const rows=[...document.querySelectorAll('tbody tr')];const index=rows.indexOf(document.activeElement);if(event.key==='ArrowDown'&&index>=0){{event.preventDefault();rows[Math.min(rows.length-1,index+1)]?.focus()}}if(event.key==='ArrowUp'&&index>=0){{event.preventDefault();rows[Math.max(0,index-1)]?.focus()}}}}); document.getElementById('evidence-summary').textContent=JSON.stringify({{completeness:report.evidence_completeness,collections:report.collection_metadata,embedded:report.embedded_evidence,source_report:report.source_report}},null,2); render();
</script></body></html>"#
    );
    fs::write(&output, html)?;
    println!("Wrote local HTML report to {}.", output.display());
    Ok(0)
}

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
                        SchemaContract::BuildInfo => include_str!("../schemas/build-info-1.json"),
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
