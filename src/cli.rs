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
    after_help = "QUICK START:\n  git slop find                       Run a safe first scan\n  git slop health                     Review repository health\n  git slop list interventions         Review maintenance candidates\n  git slop html                       Build an interactive local report\n  git slop init                       Adopt durable reports and ignore rules",
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
    /// Keep disposable state and reports under Git-private storage, without adopting `.slop/`.
    #[arg(long, conflicts_with_all = ["state_dir", "output_dir", "persist_unadopted"])]
    ephemeral: bool,
    /// Explicitly allow persistent `.slop/` output before repository adoption.
    #[arg(long, conflicts_with = "ephemeral")]
    persist_unadopted: bool,
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
    /// Estimate output format. Defaults to text on a terminal and JSON when piped.
    #[arg(long, value_enum, requires = "estimate_only")]
    format: Option<DisplayFormat>,
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
#[command(group(
    ArgGroup::new("mode")
        .args(["force", "repair", "check"])
        .multiple(false)
))]
struct InitArgs {
    /// Replace generated files atomically and keep ignored `.bak` recovery copies.
    #[arg(long)]
    force: bool,
    /// Add missing generated ignore rules without replacing repository configuration.
    #[arg(long)]
    repair: bool,
    /// Inspect adoption files without changing the repository.
    #[arg(long)]
    check: bool,
    /// Limit initialization, repair, force, or check to .slop/.gitignore.
    #[arg(long)]
    gitignore_only: bool,
}

#[derive(Debug, Args)]
struct ShowArgs {
    /// Repo-relative file or folder path.
    target_path: String,
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
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
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
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
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
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
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
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
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
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
        /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
        #[arg(long)]
        require_current: bool,
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
        /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
        #[arg(long)]
        require_current: bool,
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
        /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
        #[arg(long)]
        report: Option<PathBuf>,
        /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
        #[arg(long)]
        require_current: bool,
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
        /// Apply the removal. Without this flag the command is a read-only preview.
        #[arg(long)]
        yes: bool,
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
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
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
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Output suited for a job summary, workflow annotations, or automation.
    #[arg(long, value_enum, default_value_t = HealthFormat::Text)]
    format: HealthFormat,
    /// Maximum number of GitHub workflow annotations to emit.
    #[arg(long, default_value_t = 10)]
    max_annotations: usize,
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
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
    Migrate {
        /// Print the migrated configuration without writing it.
        #[arg(long)]
        dry_run: bool,
        /// Do not retain the existing configuration as config.yaml.bak.
        #[arg(long)]
        no_backup: bool,
    },
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
    /// Return exit 2 when the latest report is valid but stale.
    #[arg(long)]
    require_current: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DoctorFormat {
    Text,
    Json,
}

include!("cli/list_args.rs");

#[derive(Debug, Args)]
struct PruneArgs {
    /// Number of newest run snapshots to retain; defaults to output.retention_runs.
    #[arg(long)]
    keep: Option<usize>,
    /// Maximum total bytes retained; defaults to output.retention_bytes.
    #[arg(long)]
    max_bytes: Option<u64>,
    /// Explicitly request preview behavior (preview is already the default).
    #[arg(long, conflicts_with = "yes")]
    dry_run: bool,
    /// Apply the selected removals. Without this flag the command is read-only.
    #[arg(long, conflicts_with = "dry_run")]
    yes: bool,
    /// Select text, JSON, or YAML output.
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
}

include!("cli/cache_args.rs");

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
    /// Index destination. Detailed command pages use the sibling stem directory.
    /// Without an output, the complete reference is written to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct HtmlArgs {
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
    /// Destination. Defaults beside the selected report.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Embed the local source report path in the otherwise portable HTML file.
    #[arg(long)]
    include_local_paths: bool,
}

include!("cli/formats.rs");
include!("cli/support.rs");
include!("cli/support/validation.rs");
include!("cli/init.rs");
include!("cli/analysis.rs");
include!("cli/analysis_receipt.rs");
include!("cli/check.rs");
include!("cli/baseline_compare.rs");
include!("cli/reporting.rs");
include!("cli/doctor.rs");
include!("cli/listing.rs");
include!("cli/generation.rs");
include!("cli/generation/artifacts.rs");
include!("cli/generation/reference.rs");
include!("cli/generation/reference/bundle.rs");
include!("cli/entry.rs");

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
