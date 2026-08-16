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
    ReleaseManifest,
    List,
    Show,
    PromptManifest,
    Error,
    FindEstimate,
    CacheStatus,
    CachePrune,
    Baseline,
    Prune,
    CompareNdjson,
    PolicyPack,
    PolicyLock,
    AdviceInput,
    AdviceResponse,
    Advice,
    AdvisorCorpus,
    AdvisorRatings,
    AdvisorBenchmark,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BuildInfoFormat {
    Json,
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
        config::write_text_atomically(path, bytes, false)?;
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
