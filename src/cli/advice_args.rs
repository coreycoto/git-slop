fn parse_bounded_usize(value: &str, minimum: usize, maximum: usize) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{value:?} is not a positive integer"))?;
    if (minimum..=maximum).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!("value must be between {minimum} and {maximum}"))
    }
}

fn parse_max_response_bytes(value: &str) -> Result<usize, String> {
    parse_bounded_usize(value, 4_096, 4_194_304)
}

fn parse_max_context_bytes(value: &str) -> Result<usize, String> {
    parse_bounded_usize(value, 16_384, 1_048_576)
}

fn parse_max_context_tokens(value: &str) -> Result<usize, String> {
    parse_bounded_usize(value, 2_048, 32_768)
}

fn parse_max_output_tokens(value: &str) -> Result<usize, String> {
    parse_bounded_usize(value, 128, 8_192)
}

fn parse_runtime_context_tokens(value: &str) -> Result<usize, String> {
    parse_bounded_usize(value, 2_048, 40_960)
}

fn parse_excerpt_bytes(value: &str) -> Result<usize, String> {
    parse_bounded_usize(value, 512, 16_384)
}

fn parse_max_slices(value: &str) -> Result<usize, String> {
    parse_bounded_usize(value, 1, 10)
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("selector")
        .args(["path", "cluster", "relationship", "top"])
        .multiple(false)
))]
struct AdviseArgs {
    /// Report path. Advice always requires this report to match the current worktree.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Repo-relative file or folder path.
    #[arg(long, conflicts_with = "validate_artifact")]
    path: Option<String>,
    /// Relationship identifier.
    #[arg(long, conflicts_with = "validate_artifact")]
    relationship: Option<String>,
    /// Cluster identifier.
    #[arg(long, conflicts_with = "validate_artifact")]
    cluster: Option<String>,
    /// Evaluate the top N deterministic interventions, then health refactor candidates.
    #[arg(long, conflicts_with = "validate_artifact")]
    top: Option<usize>,
    /// Evaluate only this already-locked pack or rule in addition to all core invariants.
    #[arg(long = "policy", action = clap::ArgAction::Append, conflicts_with = "validate_artifact")]
    policies: Vec<String>,
    /// Emit byte-stable provider-independent advice input without model inference.
    #[arg(long, conflicts_with = "validate_artifact")]
    context_only: bool,
    /// Avoid context-cache and advice-state writes; useful for disposable benchmarks.
    #[arg(long, conflicts_with = "validate_artifact")]
    ephemeral: bool,
    /// Apply a trusted synthetic gold-case proposal (benchmark harness only).
    #[arg(long, value_enum, default_value_t = crate::advice::EvaluationScenario::Unmodified, hide = true, conflicts_with = "validate_artifact")]
    evaluation_scenario: crate::advice::EvaluationScenario,
    /// Validate and render an existing advice artifact against the current selected report.
    #[arg(long)]
    validate_artifact: Option<PathBuf>,
    /// Out-of-process reasoning provider.
    #[arg(long, value_enum, default_value_t = crate::advice::ProviderKind::OpenaiCompatible, conflicts_with = "validate_artifact")]
    provider: crate::advice::ProviderKind,
    /// Explicit OpenAI-compatible chat-completions endpoint.
    #[arg(long, conflicts_with = "validate_artifact")]
    endpoint: Option<String>,
    /// Model identity. V1 accepts only openai/gpt-oss-safeguard-20b.
    #[arg(long, conflicts_with = "validate_artifact")]
    model: Option<String>,
    /// Provider-specific served-model name; defaults to the canonical model ID.
    #[arg(long, conflicts_with = "validate_artifact")]
    runtime_model: Option<String>,
    /// Human-readable local runtime identity recorded in advice provenance.
    #[arg(long, conflicts_with = "validate_artifact")]
    runtime_label: Option<String>,
    /// Exact model artifact digest or immutable runtime model revision.
    #[arg(long, conflicts_with = "validate_artifact")]
    model_digest: Option<String>,
    /// Permit an explicitly configured non-loopback endpoint and record that choice.
    #[arg(long, conflicts_with = "validate_artifact")]
    allow_remote: bool,
    /// Reasoning effort supplied to the local provider.
    #[arg(long, value_enum, default_value_t = crate::advice::ReasoningEffort::Medium, conflicts_with = "validate_artifact")]
    reasoning: crate::advice::ReasoningEffort,
    /// Provider timeout in seconds.
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=3600), conflicts_with = "validate_artifact")]
    timeout_seconds: u64,
    /// Maximum accepted provider response size in bytes.
    #[arg(long, default_value_t = 1_048_576, value_parser = parse_max_response_bytes, conflicts_with = "validate_artifact")]
    max_response_bytes: usize,
    /// Maximum generated output tokens requested from the provider.
    #[arg(long, default_value_t = 2048, value_parser = parse_max_output_tokens, conflicts_with = "validate_artifact")]
    max_output_tokens: usize,
    /// Total provider context window. Defaults to the input and output token budgets combined.
    #[arg(long, value_parser = parse_runtime_context_tokens, conflicts_with = "validate_artifact")]
    runtime_context_tokens: Option<usize>,
    /// Maximum provider-independent context size in bytes.
    #[arg(long, default_value_t = 131_072, value_parser = parse_max_context_bytes, conflicts_with = "validate_artifact")]
    max_context_bytes: usize,
    /// Maximum estimated o200k_harmony input tokens.
    #[arg(long, default_value_t = 8192, value_parser = parse_max_context_tokens, conflicts_with = "validate_artifact")]
    max_context_tokens: usize,
    /// Maximum bytes included from each repository file.
    #[arg(long, default_value_t = 4096, value_parser = parse_excerpt_bytes, conflicts_with = "validate_artifact")]
    excerpt_bytes: usize,
    /// Maximum plan slices generated for one non-top selector.
    #[arg(long, default_value_t = 3, value_parser = parse_max_slices, conflicts_with = "validate_artifact")]
    max_slices: usize,
    /// Structured mock response used only with --provider mock.
    #[arg(long, conflicts_with = "validate_artifact")]
    mock_response: Option<PathBuf>,
    /// Render validated advice as Markdown or JSON.
    #[arg(long, value_enum, default_value_t = AdviceFormat::Markdown)]
    format: AdviceFormat,
    /// Also write the selected rendering to this repo-relative or absolute path.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AdviceFormat {
    Markdown,
    Json,
}
