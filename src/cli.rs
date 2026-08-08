use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use serde_json::{Value, json};
use sha2::Digest;

use crate::build_info;
use crate::config;
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
    /// Compare two existing schema-4 reports without rerunning the detector.
    Compare(CompareArgs),
    /// Export action-queue findings from an existing schema-4 report as SARIF.
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
    /// Generate shell completion source.
    Completions(CompletionsArgs),
    /// Write a self-contained, local, searchable HTML report.
    Html(HtmlArgs),
    /// Print version information.
    Version,
    /// Print package and source-build provenance.
    BuildInfo(BuildInfoArgs),
}

#[derive(Debug, Args)]
struct FindArgs {
    /// Acknowledge incomplete history and continue in a shallow clone.
    #[arg(long)]
    allow_shallow: bool,
    /// Analyze only this repo-relative path while retaining repository-wide Git evidence.
    #[arg(long)]
    scope: Option<String>,
    /// Suppress human progress and report-path messages.
    #[arg(long, visible_alias = "no-progress")]
    quiet: bool,
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
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Base report.json path.
    #[arg(long)]
    base: PathBuf,
    /// Head report.json path.
    #[arg(long)]
    head: PathBuf,
    /// Maximum number of changed files and queue movements to show.
    #[arg(long, default_value_t = 10)]
    top: i64,
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
    /// Compare reports with incompatible identity or analyzer metadata.
    #[arg(long)]
    force: bool,
    /// Exit 1 when an existing file worsens or a newly added file is a finding.
    #[arg(long)]
    fail_on_regression: bool,
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
}

#[derive(Debug, Args)]
struct PruneArgs {
    /// Number of newest run snapshots to retain; defaults to output.retention_runs.
    #[arg(long)]
    keep: Option<usize>,
    /// Print removals without changing files.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct CompletionsArgs {
    shell: CompletionShell,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DisplayFormat {
    Text,
    Json,
    Yaml,
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
        println!("Prompt pack path is not a directory: {}", path.display());
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
    let result = analyze::run_find_scoped(
        repo_root,
        args.allow_shallow,
        args.scope.as_deref(),
        !args.quiet,
    )?;
    if args.quiet {
        return Ok(0);
    }
    print_text(&result.terminal);
    println!("Wrote report to {}.", result.report_json.display());
    println!("Wrote YAML report to {}.", result.report_yaml.display());
    println!("Wrote summary to {}.", result.summary_md.display());
    println!(
        "Wrote repository health summary to {}.",
        result.health_md.display()
    );
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
        write_prompt_pack("explain", &payload, &loaded, output_dir)?;
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
        write_prompt_pack("plan", &payload, &loaded, output_dir)?;
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
            CheckFormat::Json => print_text(&render_json(&json!({
                "schema_version": 1,
                "command": "check",
                "report": {"schema_version": loaded.get("schema_version"), "analyzer": loaded.get("analyzer"), "repo": loaded.get("repo")},
                "boundary": {"context_band": context_band, "slop_band": slop_band},
                "passed": failures.is_empty(),
                "finding_count": failures.len(),
                "findings": failures
            }))?),
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

fn run_compare(args: CompareArgs) -> Result<i32> {
    let Some(base_report) = (match load_report_at(&args.base) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{error:#}");
            return Ok(2);
        }
    }) else {
        eprintln!("Report not found: {}", args.base.display());
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
        Some(&args.base.to_string_lossy()),
        Some(&args.head.to_string_lossy()),
        top,
        args.force,
    ) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_compare_text(&payload, top)),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&payload)?),
    }
    let regressions = payload
        .pointer("/summary/worsened_file_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        + payload
            .pointer("/summary/files/added")
            .and_then(Value::as_u64)
            .unwrap_or_default();
    Ok(if args.fail_on_regression && regressions > 0 {
        1
    } else {
        0
    })
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
            println!(
                "Configuration is valid: {}",
                config::config_path(repo_root).display()
            );
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
        ConfigCommand::Schema => print_text(&render_json(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Git Slop configuration schema 2",
            "type": "object",
            "additionalProperties": false,
            "default": config::default_config()
        }))?),
    }
    Ok(0)
}

fn run_doctor(repo_root: &Path, args: DoctorArgs) -> Result<i32> {
    let repo = git::repo_metadata(repo_root)?;
    let config_result = config::load(repo_root);
    let report_path = default_report_path(repo_root);
    let report_status = if report_path.exists() {
        match report::load_report(&report_path) {
            Ok(_) => "compatible",
            Err(_) => "invalid",
        }
    } else {
        "missing"
    };
    let tracked = git::list_tracked_files(repo_root)?.len();
    let estimated_mb = tracked.saturating_mul(24).div_ceil(1024).max(1);
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
            "valid"
        } else {
            "invalid"
        }
    );
    println!("- report: {report_status}");
    println!("- preflight: {tracked} tracked files; estimated analysis floor {estimated_mb} MiB");
    if let Some(output) = args.bundle {
        let output = if output.is_absolute() {
            output
        } else {
            repo_root.join(output)
        };
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
            "metrics": {"tracked_file_count": tracked, "estimated_memory_floor_mb": estimated_mb},
            "privacy": {"source_included": false, "raw_tokens_included": false, "absolute_paths_included": false, "author_identities_included": false, "credentials_included": false}
        });
        fs::write(&output, render_json(&payload)?)?;
        println!("Wrote redacted diagnostic bundle to {}.", output.display());
    }
    Ok(if config_result.is_err() || report_status == "invalid" {
        2
    } else {
        0
    })
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
            .get("relationships")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.values())
            .filter_map(Value::as_array)
            .flatten()
            .cloned()
            .collect(),
        ListCommand::Clusters(_) => loaded
            .get("clusters")
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
    values.truncate(filter.top);
    match filter.format {
        DisplayFormat::Json => print_text(&render_json(&Value::Array(values))?),
        DisplayFormat::Yaml => print_text(&serde_yaml::to_string(&values)?),
        DisplayFormat::Text => {
            for item in values {
                println!("{}", serde_json::to_string(&item)?);
            }
        }
    }
    Ok(0)
}

fn run_prune(repo_root: &Path, args: PruneArgs) -> Result<i32> {
    let keep = args.keep.unwrap_or_else(|| {
        config::pointer_u64(
            &config::load(repo_root).unwrap_or_else(|_| config::default_config()),
            "/output/retention_runs",
            20,
        ) as usize
    });
    let root = config::runs_dir(repo_root);
    if !root.exists() {
        println!("No run snapshots to prune.");
        return Ok(0);
    }
    let mut runs = fs::read_dir(&root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    runs.sort_by_key(|entry| std::cmp::Reverse(entry.file_name()));
    let remove = runs.into_iter().skip(keep).collect::<Vec<_>>();
    for entry in &remove {
        println!(
            "{} {}",
            if args.dry_run {
                "Would remove"
            } else {
                "Removing"
            },
            entry.path().display()
        );
        if !args.dry_run {
            fs::remove_dir_all(entry.path())?;
        }
    }
    println!(
        "{} {} old run snapshot(s); kept {keep}.",
        if args.dry_run { "Selected" } else { "Pruned" },
        remove.len()
    );
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

fn run_html(repo_root: &Path, args: HtmlArgs) -> Result<i32> {
    let (loaded, _) = match report_or_missing(repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let output = args
        .output
        .unwrap_or_else(|| config::latest_dir(repo_root).join("report.html"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string(&loaded)?.replace("</", "<\\/");
    let html = format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Git Slop local report</title><style>
:root {{ color-scheme: light dark; font: 15px system-ui,sans-serif }} body {{ margin: 2rem; max-width: 1100px }}
input,select {{ padding:.55rem; margin:0 .5rem .75rem 0 }} table {{ width:100%; border-collapse:collapse }}
th,td {{ text-align:left; padding:.5rem; border-bottom:1px solid #8885 }} th {{ cursor:pointer }}
code {{ overflow-wrap:anywhere }} details {{ margin:1rem 0 }} .muted {{ opacity:.7 }}
</style></head><body><h1>Git Slop local report</h1><p id="descriptor" class="muted"></p>
<input id="query" type="search" placeholder="Search paths"><select id="profile"><option value="">All profiles</option></select>
<select id="severity"><option value="">All maintenance bands</option><option>critical</option><option>high</option><option>moderate</option><option>low</option></select>
<p id="count"></p><table><thead><tr><th data-key="path">Path</th><th data-key="profile">Profile</th><th data-key="language">Language</th><th data-key="slop_band">Maintenance</th><th data-key="context_band">Context</th><th data-key="slop_score">Score</th><th data-key="tokens">Tokens</th></tr></thead><tbody id="rows"></tbody></table>
<details><summary>Relationships</summary><pre id="relationships"></pre></details>
<script id="report" type="application/json">{payload}</script><script>
const report=JSON.parse(document.getElementById('report').textContent); let sortKey='slop_score', ascending=false;
const files=report.files||[]; const esc=v=>String(v??'').replace(/[&<>"']/g,c=>({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c]));
document.getElementById('descriptor').textContent=`${{report.repo?.repo_name||'repository'}} · ${{report.generated_at||'unknown time'}} · schema ${{report.schema_version}}`;
const profile=document.getElementById('profile'); [...new Set(files.map(f=>f.profile).filter(Boolean))].sort().forEach(v=>profile.insertAdjacentHTML('beforeend',`<option>${{esc(v)}}</option>`));
function render() {{ const q=document.getElementById('query').value.toLowerCase(), p=profile.value, s=document.getElementById('severity').value;
 const selected=files.filter(f=>(!q||String(f.path).toLowerCase().includes(q))&&(!p||f.profile===p)&&(!s||f.slop_band===s)).sort((a,b)=>{{const x=a[sortKey],y=b[sortKey]; return (typeof x==='number'?x-y:String(x??'').localeCompare(String(y??'')))*(ascending?1:-1)}});
 document.getElementById('count').textContent=`${{selected.length}} of ${{files.length}} files`;
 document.getElementById('rows').innerHTML=selected.map(f=>`<tr><td><code>${{esc(f.path)}}</code></td><td>${{esc(f.profile)}}</td><td>${{esc(f.language)}}</td><td>${{esc(f.slop_band)}}</td><td>${{esc(f.context_band)}}</td><td>${{esc(f.slop_score)}}</td><td>${{esc(f.tokens)}}</td></tr>`).join(''); }}
document.querySelectorAll('input,select').forEach(el=>el.addEventListener('input',render)); document.querySelectorAll('th').forEach(el=>el.addEventListener('click',()=>{{ascending=sortKey===el.dataset.key?!ascending:false;sortKey=el.dataset.key;render()}}));
const rel=report.relationships||report.overlays?.organization_health?.relationships||{{}}; document.getElementById('relationships').textContent=JSON.stringify(rel,null,2); render();
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
        Command::Compare(args) => run_compare(args),
        Command::Sarif(args) => run_sarif(repo_root, args),
        Command::Health(args) => run_health(repo_root, args),
        Command::Config(args) => run_config(repo_root, args),
        Command::Doctor(args) => run_doctor(repo_root, args),
        Command::List(args) => run_list(repo_root, args),
        Command::Prune(args) => run_prune(repo_root, args),
        Command::Completions(args) => run_completions(args),
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
    let repo_root = match git::resolve_repo_root_from(cli.repo.as_deref()) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error:#}");
            return 3;
        }
    };
    match execute(&repo_root, cli.command) {
        Ok(code) => code,
        Err(error) => {
            let rendered = format!("{error:#}");
            eprintln!("{rendered}");
            if rendered.contains("memory_budget_mb") || rendered.contains("analysis bounded") {
                4
            } else if rendered.contains("config")
                || rendered.contains("schema")
                || rendered.contains("unsupported tokenization")
            {
                2
            } else {
                3
            }
        }
    }
}
