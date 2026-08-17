fn execute(repo_root: &Path, command: Command) -> Result<i32> {
    match command {
        Command::Init(args) => run_init(repo_root, args),
        Command::Find(args) => run_find(repo_root, args),
        Command::Show(args) => run_show(repo_root, args),
        Command::Explain(args) => run_explain(repo_root, args),
        Command::Plan(args) => run_plan(repo_root, args),
        Command::Policy(args) => run_policy(repo_root, args),
        Command::Advise(args) => run_advise(repo_root, *args),
        Command::Check(args) => run_check(repo_root, args),
        Command::Compare(args) => run_compare(repo_root, args),
        Command::Baseline(args) => run_baseline(repo_root, args),
        Command::Report(args) => run_report(repo_root, args),
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
                        SchemaContract::Compare => include_str!("../../schemas/compare-1.json"),
                        SchemaContract::Explain => include_str!("../../schemas/explain-2.json"),
                        SchemaContract::Plan => include_str!("../../schemas/plan-2.json"),
                        SchemaContract::Sarif => include_str!("../../schemas/sarif-1.json"),
                        SchemaContract::Health => include_str!("../../schemas/health-1.json"),
                        SchemaContract::Check => include_str!("../../schemas/check-1.json"),
                        SchemaContract::Doctor => include_str!("../../schemas/doctor-1.json"),
                        SchemaContract::BuildInfo => include_str!("../../schemas/build-info-2.json"),
                        SchemaContract::ReleaseManifest => {
                            include_str!("../../schemas/release-manifest-3.json")
                        }
                        SchemaContract::List => include_str!("../../schemas/list-1.json"),
                        SchemaContract::Show => include_str!("../../schemas/show-1.json"),
                        SchemaContract::PromptManifest => {
                            include_str!("../../schemas/prompt-manifest-1.json")
                        }
                        SchemaContract::Error => include_str!("../../schemas/error-1.json"),
                        SchemaContract::FindEstimate => {
                            include_str!("../../schemas/find-estimate-1.json")
                        }
                        SchemaContract::CacheStatus => {
                            include_str!("../../schemas/cache-status-1.json")
                        }
                        SchemaContract::CachePrune => {
                            include_str!("../../schemas/cache-prune-1.json")
                        }
                        SchemaContract::Baseline => include_str!("../../schemas/baseline-1.json"),
                        SchemaContract::Prune => include_str!("../../schemas/prune-1.json"),
                        SchemaContract::CompareNdjson => {
                            include_str!("../../schemas/compare-ndjson-1.json")
                        }
                        SchemaContract::PolicyPack => {
                            include_str!("../../schemas/policy-pack-1.json")
                        }
                        SchemaContract::PolicyLock => {
                            include_str!("../../schemas/policy-lock-1.json")
                        }
                        SchemaContract::AdviceInput => {
                            include_str!("../../schemas/advice-input-1.json")
                        }
                        SchemaContract::AdviceResponse => {
                            include_str!("../../schemas/advice-response-1.json")
                        }
                        SchemaContract::Advice => include_str!("../../schemas/advice-1.json"),
                        SchemaContract::AdvisorCorpus => {
                            include_str!("../../schemas/advisor-corpus-1.json")
                        }
                        SchemaContract::AdvisorRatings => {
                            include_str!("../../schemas/advisor-ratings-1.json")
                        }
                        SchemaContract::AdvisorThresholds => {
                            include_str!("../../schemas/advisor-thresholds-1.json")
                        }
                        SchemaContract::AdvisorBenchmark => {
                            include_str!("../../schemas/advisor-benchmark-1.json")
                        }
                        SchemaContract::AdvisorCapacity => {
                            include_str!("../../schemas/advisor-capacity-1.json")
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
        Command::Show(args) => args.report.is_none() || args.require_current,
        Command::Compare(args) => args.base_ref.is_some() || args.baseline.is_some(),
        Command::Baseline(_) => true,
        Command::Policy(_) => true,
        Command::Advise(_) => true,
        Command::Explain(args) => {
            args.report.is_none() || args.include_repository_context || args.require_current
        }
        Command::Plan(args) => {
            args.report.is_none() || args.include_repository_context || args.require_current
        }
        Command::Check(args) => args.report.is_none() || args.require_current,
        Command::Sarif(args) => args.report.is_none() || args.require_current,
        Command::Health(args) => args.report.is_none() || args.require_current,
        Command::Html(args) => args.report.is_none() || args.require_current,
        Command::List(args) => {
            let output = match &args.command {
                ListCommand::PolicyFailures(args)
                | ListCommand::Interventions(args)
                | ListCommand::Observations(args)
                | ListCommand::HealthFindings(args)
                | ListCommand::Findings(args) => &args.output,
                ListCommand::Relationships(args) => &args.output,
                ListCommand::Clusters(args) => &args.output,
                ListCommand::Profiles(args) => &args.output,
            };
            output.report.is_none() || output.require_current
        }
        _ => true,
    }
}

pub fn run() -> i32 {
    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if raw_args.len() == 1 {
        println!(
            "Git Slop finds repository surfaces that cost too much context.\n\n\
             QUICK START\n  git slop find                 Run a safe, cacheable first scan\n  \
             git slop health               Review the current health snapshot\n  \
             git slop list interventions   Review bounded maintenance candidates\n  \
             git slop explain --top 5      Understand why candidates surfaced\n\n\
             INSPECT\n  show · explain · plan · list · html · doctor\n\n\
             AUTOMATION\n  check · compare · baseline · sarif\n\n\
             ADOPTION AND STATE\n  init · config · cache · prune\n\n\
             POLICY-GUIDED ADVICE\n  policy · advise\n\n\
             GENERATE AND INFO\n  completions · man · reference · schema · version · build-info\n\n\
             Run `git slop --help` for every command and option."
        );
        return 0;
    }
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
    let repo_root = if cli.repo.is_some() || command_requires_repository(&cli.command) {
        match git::resolve_repo_root_from(cli.repo.as_deref()) {
            Ok(root) => root,
            Err(error) => {
                let requested_path = cli.repo.clone().or_else(|| std::env::current_dir().ok());
                let requested_display = requested_path.as_deref().map_or_else(
                    || "the current directory".to_string(),
                    |path| path.display().to_string(),
                );
                render_runtime_error(
                    error_format,
                    &ClassifiedError::new(
                        ErrorKind::Repository,
                        "repository_not_found",
                        format!(
                            "Not inside a Git repository: {requested_display}. Run this command from a repository, or pass `--repo <PATH>`."
                        ),
                    )
                    .at("/repo")
                    .with_details(json!({"path": requested_path, "cause": format!("{error:#}")})),
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
        "policy",
        "advise",
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
