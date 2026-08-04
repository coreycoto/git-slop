use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use serde_json::Value;

use crate::build_info;
use crate::config;
use crate::health;
use crate::report;
use crate::report_ops::{
    ExplainSelector, PlanSelector, compare_payload, explain_payload, failing_records,
    health_json_payload, plan_payload, render_compare_text, render_explain_text,
    render_github_annotations, render_json, render_plan_text, sarif_payload, show_payload,
    write_prompt_pack,
};
use crate::{PROJECT_NAME, VERSION, analyze, git};

#[derive(Debug, Parser)]
#[command(
    name = "git-slop",
    about = "Find the files that cost too much context.",
    disable_version_flag = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scaffold .slop/ config, ignore rules, and state directories.
    Init(InitArgs),
    /// Scan the repository and generate hotspot reports.
    Find,
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
    /// Print version information.
    Version,
    /// Print package and source-build provenance.
    BuildInfo(BuildInfoArgs),
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
    #[arg(long, value_enum, default_value_t = HealthFormat::Markdown)]
    format: HealthFormat,
    /// Maximum number of GitHub workflow annotations to emit.
    #[arg(long, default_value_t = 10)]
    max_annotations: usize,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DisplayFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HealthFormat {
    Markdown,
    Github,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BuildInfoFormat {
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
    Ok(match load_default_report(repo_root, explicit_report)? {
        Some(loaded) => Ok(loaded),
        None => {
            println!("Report not found: {}", fallback.display());
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
    println!("{error}");
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

fn run_init(args: InitArgs) -> Result<i32> {
    let repo_root = git::resolve_repo_root()?;
    let result = config::initialize(&repo_root, args.force)?;
    println!(
        "Initialized {} ({}).",
        relative_display(&config::config_path(&repo_root), &repo_root),
        result.config
    );
    println!(
        "Initialized {} ({}).",
        relative_display(&config::slop_dir(&repo_root).join(".gitignore"), &repo_root),
        result.gitignore
    );
    println!("Ensured .slop/latest/, .slop/runs/, and .slop/cache/ exist.");
    Ok(0)
}

fn run_find() -> Result<i32> {
    let result = analyze::run_find()?;
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

fn run_show(args: ShowArgs) -> Result<i32> {
    let repo_root = git::resolve_repo_root()?;
    let (loaded, report_path) = match report_or_missing(&repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let target = selector_path(&repo_root, &args.target_path);
    let Some(payload) = show_payload(&loaded, &target) else {
        println!(
            "No record found for '{}' in {}.",
            args.target_path,
            report_path.display()
        );
        return Ok(2);
    };
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => {
            let yaml = serde_yaml::to_string(&payload)?;
            print_text(&yaml);
        }
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

fn run_explain(args: ExplainArgs) -> Result<i32> {
    let repo_root = git::resolve_repo_root()?;
    let (loaded, _) = match report_or_missing(&repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let selector = match explain_selector(&args, &repo_root) {
        Ok(selector) => selector,
        Err(code) => {
            println!("--top must be greater than zero.");
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

fn run_plan(args: PlanArgs) -> Result<i32> {
    let repo_root = git::resolve_repo_root()?;
    let (loaded, _) = match report_or_missing(&repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let Some(max_slices) = usize::try_from(args.max_slices)
        .ok()
        .filter(|count| *count > 0)
    else {
        println!("--max-slices must be greater than zero.");
        return Ok(2);
    };
    let payload = match plan_payload(&loaded, plan_selector(&args, &repo_root), max_slices) {
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
    }
    Ok(0)
}

fn run_check(args: CheckArgs) -> Result<i32> {
    let repo_root = git::resolve_repo_root()?;
    let (loaded, _) = match report_or_missing(&repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let loaded_config = config::load(&repo_root)?;
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
    let Some(base_report) = load_report_at(&args.base)? else {
        println!("Report not found: {}", args.base.display());
        return Ok(2);
    };
    let Some(head_report) = load_report_at(&args.head)? else {
        println!("Report not found: {}", args.head.display());
        return Ok(2);
    };
    let Some(top) = usize::try_from(args.top).ok().filter(|count| *count > 0) else {
        println!("--top must be greater than zero.");
        return Ok(2);
    };
    let payload = match compare_payload(
        &base_report,
        &head_report,
        Some(&args.base.to_string_lossy()),
        Some(&args.head.to_string_lossy()),
        top,
    ) {
        Ok(payload) => payload,
        Err(error) => return usage_error(error),
    };
    match args.format {
        DisplayFormat::Json => print_text(&render_json(&payload)?),
        DisplayFormat::Text => print_text(&render_compare_text(&payload, top)),
    }
    Ok(0)
}

fn run_sarif(args: SarifArgs) -> Result<i32> {
    let repo_root = git::resolve_repo_root()?;
    let (loaded, report_path) = match report_or_missing(&repo_root, args.report.as_deref())? {
        Ok(value) => value,
        Err(code) => return Ok(code),
    };
    let top = match args.top {
        None => None,
        Some(value) => match usize::try_from(value).ok().filter(|count| *count > 0) {
            Some(value) => Some(value),
            None => {
                println!("--top must be greater than zero.");
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

fn run_health(args: HealthArgs) -> Result<i32> {
    let repo_root = git::resolve_repo_root()?;
    let (mut loaded, _) = match report_or_missing(&repo_root, args.report.as_deref())? {
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

fn execute(command: Command) -> Result<i32> {
    match command {
        Command::Init(args) => run_init(args),
        Command::Find => run_find(),
        Command::Show(args) => run_show(args),
        Command::Explain(args) => run_explain(args),
        Command::Plan(args) => run_plan(args),
        Command::Check(args) => run_check(args),
        Command::Compare(args) => run_compare(args),
        Command::Sarif(args) => run_sarif(args),
        Command::Health(args) => run_health(args),
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
    match execute(cli.command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error:#}");
            1
        }
    }
}
