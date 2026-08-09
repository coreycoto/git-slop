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
    ExplainSelector, PlanSelector, compare_payload_with_force, explain_payload, failing_records,
    health_json_payload, plan_payload, render_compare_text, render_explain_text,
    render_github_annotations, render_json, render_plan_text, render_show_text, sarif_payload,
    show_payload, write_prompt_pack,
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

#[derive(Debug, Args)]
struct SchemaArgs {
    #[arg(value_enum)]
    contract: SchemaContract,
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
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
    /// Write a deterministic local-model prompt pack to this directory.
    #[arg(long)]
    prompt_pack: Option<PathBuf>,
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
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
    /// Write a deterministic local-model prompt pack to this directory.
    #[arg(long)]
    prompt_pack: Option<PathBuf>,
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
    #[arg(long, value_enum, default_value_t = CheckFormat::Text)]
    format: CheckFormat,
    /// Include complete finding records in JSON output.
    #[arg(long)]
    details: bool,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Base report.json path.
    #[arg(
        long,
        required_unless_present = "base_ref",
        conflicts_with = "base_ref"
    )]
    base: Option<PathBuf>,
    /// Safely resolve and scan this Git revision in an isolated worktree.
    #[arg(long, conflicts_with = "base")]
    base_ref: Option<String>,
    /// Head report.json path.
    #[arg(long, default_value = ".slop/latest/report.json")]
    head: PathBuf,
    /// Apply the head repository's scope to an isolated --base-ref scan.
    #[arg(long)]
    scope: Option<String>,
    /// Permit incomplete history in an isolated --base-ref scan.
    #[arg(long)]
    allow_shallow: bool,
    /// Maximum number of changed files and queue movements to show.
    #[arg(long, default_value_t = 10)]
    top: i64,
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
    /// Exit 1 when an existing file worsens or a newly added file is a finding.
    #[arg(long)]
    fail_on_regression: bool,
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
        #[arg(value_name = "REPORT_JSON")]
        path: PathBuf,
        /// Accept schema 4 as migration input and validate its normalized schema-5 form.
        #[arg(long)]
        allow_legacy: bool,
    },
    /// Migrate a schema-4 report to normalized schema 5.
    Migrate {
        #[arg(value_name = "REPORT_JSON")]
        path: PathBuf,
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
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    language: Option<String>,
    #[arg(long, visible_alias = "class")]
    classification: Option<String>,
    #[arg(long)]
    severity: Option<String>,
    #[arg(long, default_value_t = 50)]
    top: usize,
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
    #[arg(long, value_name = "PATH")]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: CacheCommand,
}

#[derive(Debug, Subcommand)]
enum CacheCommand {
    Status {
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    Prune {
        #[arg(long, default_value_t = 10_000)]
        max_entries: usize,
        #[arg(long, default_value_t = 536_870_912)]
        max_bytes: u64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
}

#[derive(Debug, Args)]
struct CompletionsArgs {
    shell: CompletionShell,
}

#[derive(Debug, Args)]
struct ManArgs {
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ReferenceArgs {
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
    value
        .chars()
        .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
        .collect()
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

fn report_or_missing(
    repo_root: &Path,
    explicit_report: Option<&Path>,
) -> Result<Result<(Value, PathBuf), i32>> {
    let fallback = explicit_report
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_report_path(repo_root));
    let loaded = match load_default_report(repo_root, explicit_report) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("{error:#}");
            return Ok(Err(2));
        }
    };
    Ok(match loaded {
        Some(loaded) => Ok(loaded),
        None => {
            eprintln!(
                "Report not found: {}\nRun `git slop find` to generate it.",
                fallback.display()
            );
            Err(2)
        }
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
    eprintln!("{error}");
    Ok(2)
}

fn ensure_prompt_pack_target(path: &Path) -> Result<Result<(), i32>> {
    if path.exists() && !path.is_dir() {
        eprintln!("Prompt pack path is not a directory: {}", path.display());
        Ok(Err(2))
    } else {
        Ok(Ok(()))
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
    let (loaded, report_path) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let target = selector_path(repo_root, &args.target_path);
    let Some(payload) = show_payload(&loaded, &target) else {
        eprintln!(
            "No record found for '{}' in {}.",
            args.target_path,
            report_path.display()
        );
        return Ok(2);
    };
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_show_text(&payload)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    Ok(0)
}

fn explain_selector(args: &ExplainArgs, repo_root: &Path) -> Result<ExplainSelector, i32> {
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
            None => Err(2),
        }
    }
}

fn run_explain(repo_root: &Path, args: ExplainArgs) -> Result<i32> {
    if args.include_repository_context && !(256..=4096).contains(&args.excerpt_bytes) {
        eprintln!("--excerpt-bytes must be between 256 and 4096.");
        return Ok(2);
    }
    let (loaded, _) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let selector = match explain_selector(&args, repo_root) {
        Ok(selector) => selector,
        Err(code) => {
            eprintln!("--top must be greater than zero.");
            return Ok(code);
        }
    };
    let payload = match explain_payload(&loaded, Some(selector)) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    if let Some(output_dir) = args.prompt_pack.as_deref() {
        if let Err(code) = ensure_prompt_pack_target(output_dir)? {
            return Ok(code);
        }
        write_prompt_pack(
            "explain",
            &payload,
            &loaded,
            output_dir,
            args.include_repository_context.then_some(repo_root),
            args.excerpt_bytes,
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
        eprintln!("--excerpt-bytes must be between 256 and 4096.");
        return Ok(2);
    }
    let (loaded, _) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let Some(max_slices) = usize::try_from(args.max_slices)
        .ok()
        .filter(|count| *count > 0)
    else {
        eprintln!("--max-slices must be greater than zero.");
        return Ok(2);
    };
    let payload = match plan_payload(&loaded, plan_selector(&args, repo_root), max_slices) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    if let Some(output_dir) = args.prompt_pack.as_deref() {
        if let Err(code) = ensure_prompt_pack_target(output_dir)? {
            return Ok(code);
        }
        write_prompt_pack(
            "plan",
            &payload,
            &loaded,
            output_dir,
            args.include_repository_context.then_some(repo_root),
            args.excerpt_bytes,
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
    let (loaded, _) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
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
    let failures = failing_records(&loaded, Some(context_band), Some(slop_band));
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
                });
                if args.details {
                    payload["findings"] = json!(failures);
                }
                print_text(&render_json(&payload)?);
            }
            CheckFormat::Github => {
                for failure in &failures {
                    let path = safe_terminal(
                        failure
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    );
                    println!(
                        "::error file={}::Git Slop context={} slop={} score={}",
                        path.replace('%', "%25").replace(',', "%2C"),
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
            failure
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default(),
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
        "summary": payload.get("summary"),
        "pagination": payload.get("pagination"),
        "baseline_status": payload.get("baseline_status")
    }))?];
    for (key, record_type) in [
        ("file_deltas", "file_delta"),
        ("folder_deltas", "folder_delta"),
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

fn run_compare(repo_root: &Path, args: CompareArgs) -> Result<i32> {
    let materialized = if let Some(reference) = args.base_ref.as_deref() {
        Some(crate::baseline::MaterializedBaseline::create(
            repo_root,
            reference,
            args.scope.clone(),
            args.allow_shallow,
        )?)
    } else {
        None
    };
    let base_path = args
        .base
        .as_deref()
        .or_else(|| {
            materialized
                .as_ref()
                .map(|value| value.report_path.as_path())
        })
        .expect("Clap requires --base or --base-ref");
    let Some(base_report) = (match load_report_at(base_path) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{error:#}");
            return Ok(2);
        }
    }) else {
        eprintln!("Report not found: {}", base_path.display());
        return Ok(2);
    };
    let Some(head_report) = (match load_report_at(&args.head) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{error:#}");
            return Ok(2);
        }
    }) else {
        eprintln!("Report not found: {}", args.head.display());
        return Ok(2);
    };
    let Some(top) = usize::try_from(args.top).ok().filter(|count| *count > 0) else {
        eprintln!("--top must be greater than zero.");
        return Ok(2);
    };
    let payload = match compare_payload_with_force(
        &base_report,
        &head_report,
        Some(&base_path.to_string_lossy()),
        Some(&args.head.to_string_lossy()),
        top,
        args.force,
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
                Err(error) => {
                    eprintln!("{error:#}");
                    Ok(2)
                }
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
    let (loaded, report_path) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let top = match args.top {
        None => None,
        Some(value) => match usize::try_from(value).ok().filter(|count| *count > 0) {
            Some(value) => Some(value),
            None => {
                eprintln!("--top must be greater than zero.");
                return Ok(2);
            }
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
    let (mut loaded, _) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
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

fn run_config(repo_root: &Path, args: ConfigArgs) -> Result<i32> {
    match args.command {
        ConfigCommand::Show { effective } => {
            if effective {
                print_text(&serde_yaml::to_string(&config::load(repo_root)?)?);
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
            config::load(repo_root)?;
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
            let diff = diff_values(&config::load(repo_root)?, &config::default_config());
            print_text(&serde_yaml::to_string(&diff)?);
        }
        ConfigCommand::Migrate => {
            let effective = config::load(repo_root)?;
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
    let tracked_paths = git::list_tracked_files(repo_root)?
        .into_iter()
        .filter(|path| {
            args.scope
                .as_deref()
                .is_none_or(|scope| path == scope || path.starts_with(&format!("{scope}/")))
        })
        .collect::<Vec<_>>();
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
    let diagnostic = json!({
        "schema_version": 1,
        "command": "doctor",
        "status": if config_result.is_err() || report_status == "invalid" || resource_status == "over_memory_budget" { "error" } else { "ready" },
        "repository": {"name": repo.repo_name, "branch": repo.branch, "shallow": repo.is_shallow, "detached": repo.detached_head, "clean": repo.worktree_clean},
        "config": {"status": if config_result.is_err() { "invalid" } else if config_exists { "valid" } else { "using_defaults" }, "path": config::config_path(repo_root)},
        "report": {"status": report_status, "path": report_path},
        "estimate": estimate,
        "resource_status": resource_status,
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

fn matches_list_filter(item: &Value, args: &ListFilterArgs) -> bool {
    args.path.as_ref().is_none_or(|path| {
        item.get("path")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with(path))
    }) && args
        .profile
        .as_ref()
        .is_none_or(|value| item.get("profile").and_then(Value::as_str) == Some(value))
        && args
            .language
            .as_ref()
            .is_none_or(|value| item.get("language").and_then(Value::as_str) == Some(value))
        && args.classification.as_ref().is_none_or(|value| {
            item.get("classification")
                .or_else(|| item.get("class"))
                .and_then(Value::as_str)
                == Some(value)
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
    let (loaded, _) = match report_or_missing(repo_root, filter.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
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
    values.retain(|item| matches_list_filter(item, filter));
    let total = values.len();
    values.truncate(filter.top);
    let returned = values.len();
    match filter.format {
        DisplayFormat::Json => print_text(&render_json(&json!({
            "schema_version": 1,
            "command": "list",
            "kind": match &args.command {
                ListCommand::Findings(_) => "findings",
                ListCommand::Relationships(_) => "relationships",
                ListCommand::Clusters(_) => "clusters",
                ListCommand::Profiles(_) => "profiles",
            },
            "items": values,
            "collection": {"total": total, "returned": returned, "limit": filter.top, "truncated": returned < total}
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
                "\nReturned {returned} of {total} matching record(s).{}",
                if returned < total {
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
            format,
        } => (
            crate::cache::prune(&state_root, max_entries, max_bytes, dry_run)?,
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
    let (loaded, report_path) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
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
        "files": loaded.get("files"),
        "action_queue": loaded.get("action_queue"),
        "health": {
            "summary": loaded.pointer("/health/summary"),
            "findings": loaded.pointer("/health/findings")
        },
        "organization": {
            "relationships": loaded.pointer("/overlays/organization_health/relationships"),
            "clusters": loaded.pointer("/overlays/organization_health/clusters")
        },
        "source_report": report_path
    }))?
    .replace("</", "<\\/");
    let html = format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Git Slop local report</title><style>
:root {{ color-scheme: light dark; font: 15px system-ui,sans-serif }} body {{ margin: 2rem; max-width: 1100px }}
input,select {{ padding:.55rem; margin:0 .5rem .75rem 0 }} table {{ width:100%; border-collapse:collapse }}
th,td {{ text-align:left; padding:.5rem; border-bottom:1px solid #8885 }} th button {{ all:unset;cursor:pointer;font-weight:700 }}
code {{ overflow-wrap:anywhere }} details {{ margin:1rem 0 }} .muted {{ opacity:.7 }} .sr {{ position:absolute;left:-10000px }}
.views button[aria-pressed="true"] {{ font-weight:700;text-decoration:underline }} tr:target {{ outline:2px solid currentColor }}
</style></head><body><h1>Git Slop local report</h1><p id="descriptor" class="muted"></p>
<nav class="views" aria-label="Report view"><button data-view="files" aria-pressed="true">Files</button> <button data-view="queue" aria-pressed="false">Action queue</button> <button data-view="health" aria-pressed="false">Health findings</button></nav>
<label for="query" class="sr">Search paths</label><input id="query" type="search" placeholder="Search paths"><label for="profile" class="sr">Profile</label><select id="profile"><option value="">All profiles</option></select>
<label for="severity" class="sr">Maintenance band</label><select id="severity"><option value="">All maintenance bands</option><option>critical</option><option>high</option><option>moderate</option><option>low</option></select>
<p id="count" aria-live="polite"></p><button id="previous" type="button">Previous</button><button id="next" type="button">Next</button><table><caption class="sr">Git Slop records</caption><thead><tr><th scope="col"><button data-key="path">Path</button></th><th scope="col"><button data-key="profile">Profile</button></th><th scope="col"><button data-key="language">Language</button></th><th scope="col"><button data-key="slop_band">Maintenance</button></th><th scope="col"><button data-key="context_band">Context</button></th><th scope="col"><button data-key="slop_score">Score</button></th><th scope="col"><button data-key="tokens">Tokens</button></th></tr></thead><tbody id="rows"></tbody></table>
<details id="file-detail"><summary>Selected record details</summary><pre id="detail"></pre></details>
<details><summary>Relationships</summary><pre id="relationships"></pre></details>
<script id="report" type="application/json">{payload}</script><script>
const report=JSON.parse(document.getElementById('report').textContent), params=new URLSearchParams(location.search); let view=params.get('view')||'files', sortKey=params.get('sort')||'slop_score', ascending=params.get('dir')==='asc', page=Number(params.get('page')||0); const pageSize=100;
const files=report.files||[], queue=report.action_queue||[], findings=report.health?.findings||[]; const esc=v=>String(v??'').replace(/[&<>"']/g,c=>({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c]));
document.getElementById('descriptor').textContent=`${{report.repo?.repo_name||'repository'}} · ${{report.generated_at||'unknown time'}} · schema ${{report.schema_version}}`;
const profile=document.getElementById('profile'); [...new Set(files.map(f=>f.profile).filter(Boolean))].sort().forEach(v=>profile.insertAdjacentHTML('beforeend',`<option>${{esc(v)}}</option>`));
document.getElementById('query').value=params.get('q')||''; profile.value=params.get('profile')||''; document.getElementById('severity').value=params.get('band')||'';
function records() {{ return view==='queue'?queue:view==='health'?findings:files }}
function syncUrl() {{ const p=new URLSearchParams(); for (const [k,v] of Object.entries({{view,q:document.getElementById('query').value,profile:profile.value,band:document.getElementById('severity').value,sort:sortKey,dir:ascending?'asc':'desc',page}})) if(v!==''&&v!==0)p.set(k,v); history.replaceState(null,'',`${{location.pathname}}?${{p}}${{location.hash}}`) }}
function render() {{ const q=document.getElementById('query').value.toLowerCase(), p=profile.value, s=document.getElementById('severity').value, source=records();
 document.querySelectorAll('[data-view]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.view===view)));
 const selected=source.filter(f=>(!q||String(f.path||f.id).toLowerCase().includes(q))&&(!p||f.profile===p)&&(!s||(f.slop_band||f.severity)===s)).sort((a,b)=>{{const x=a[sortKey],y=b[sortKey]; return (typeof x==='number'?x-y:String(x??'').localeCompare(String(y??'')))*(ascending?1:-1)}});
 const pages=Math.max(1,Math.ceil(selected.length/pageSize)); page=Math.min(page,pages-1); const visible=selected.slice(page*pageSize,(page+1)*pageSize);
 document.getElementById('count').textContent=`${{selected.length}} of ${{source.length}} ${{view.replace('_',' ')}} records · page ${{page+1}} of ${{pages}}`;
 document.getElementById('previous').disabled=page===0; document.getElementById('next').disabled=page+1>=pages;
 document.getElementById('rows').innerHTML=visible.map((f,i)=>`<tr id="record-${{page*pageSize+i}}" data-index="${{page*pageSize+i}}"><td><button class="record"><code>${{esc(f.path||f.id)}}</code></button></td><td>${{esc(f.profile||f.kind)}}</td><td>${{esc(f.language)}}</td><td>${{esc(f.slop_band||f.severity)}}</td><td>${{esc(f.context_band)}}</td><td>${{esc(f.slop_score||f.evidence_score)}}</td><td>${{esc(f.tokens)}}</td></tr>`).join('');
 document.querySelectorAll('.record').forEach((button,i)=>button.addEventListener('click',()=>{{document.getElementById('detail').textContent=JSON.stringify(visible[i],null,2);document.getElementById('file-detail').open=true;location.hash=`record-${{page*pageSize+i}}`}})); syncUrl(); }}
document.querySelectorAll('input,select').forEach(el=>el.addEventListener('input',()=>{{page=0;render()}})); document.querySelectorAll('th button').forEach(el=>el.addEventListener('click',()=>{{ascending=sortKey===el.dataset.key?!ascending:true;sortKey=el.dataset.key;page=0;render()}}));
document.querySelectorAll('[data-view]').forEach(el=>el.addEventListener('click',()=>{{view=el.dataset.view;page=0;render()}})); document.getElementById('previous').addEventListener('click',()=>{{page=Math.max(0,page-1);render()}}); document.getElementById('next').addEventListener('click',()=>{{page+=1;render()}});
document.getElementById('relationships').parentElement.addEventListener('toggle',e=>{{if(e.target.open&&!e.target.dataset.loaded){{e.target.dataset.loaded='true';document.getElementById('relationships').textContent=JSON.stringify(report.organization?.relationships||{{}},null,2)}}}}); render();
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
            let source = match args.contract {
                SchemaContract::Report => {
                    return {
                        print_text(&render_json(&report::schema())?);
                        Ok(0)
                    };
                }
                SchemaContract::Config => {
                    return {
                        print_text(&render_json(&config::schema())?);
                        Ok(0)
                    };
                }
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
            };
            let value: Value = serde_json::from_str(source)?;
            print_text(&render_json(&value)?);
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
        Command::Compare(args) => args.base_ref.is_some(),
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
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return code;
        }
    };
    let error_format = cli.error_format;
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
            render_runtime_error(error_format, classified);
            classified.kind.exit_code()
        }
    }
}

fn render_runtime_error(format: ErrorFormat, error: &ClassifiedError) {
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
                    "message": error.message
                }
            }))
            .unwrap_or_else(|_| {
                "{\"schema_version\":1,\"error\":{\"code\":\"serialization_failed\"}}".to_string()
            })
        ),
    }
}
