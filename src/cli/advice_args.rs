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
    /// Explicitly request experimental model inference after every safety gate passes.
    #[arg(long, hide = true, conflicts_with_all = ["context_only", "validate_artifact"])]
    infer: bool,
    /// Avoid context-cache and advice-state writes; useful for disposable benchmarks.
    #[arg(long, conflicts_with = "validate_artifact")]
    ephemeral: bool,
    /// Apply a trusted synthetic gold-case proposal (benchmark harness only).
    #[arg(long, value_enum, default_value_t = crate::advice::EvaluationScenario::Unmodified, hide = true, conflicts_with = "validate_artifact")]
    evaluation_scenario: crate::advice::EvaluationScenario,
    /// Validate and render an existing advice artifact against the current selected report.
    #[arg(long)]
    validate_artifact: Option<PathBuf>,
    /// Explicit out-of-process reasoning provider. Required with --infer.
    #[arg(long, value_enum, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    provider: Option<crate::advice::ProviderKind>,
    /// Explicit loopback provider endpoint. Remote endpoints are refused in V1.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    endpoint: Option<String>,
    /// Explicit model identity. V1 accepts only openai/gpt-oss-safeguard-20b.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    model: Option<String>,
    /// Explicit provider-specific served-model name.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    runtime_model: Option<String>,
    /// Explicit human-readable runtime name and version recorded in provenance.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    runtime_label: Option<String>,
    /// Exact model artifact digest or immutable runtime model revision.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    model_digest: Option<String>,
    /// Exact model artifact size used by the fail-closed capacity preflight.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    model_size_bytes: Option<u64>,
    /// Conservative peak host-memory estimate for the configured model and context.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    estimated_peak_memory_bytes: Option<u64>,
    /// Confirm the displayed resource contract after independently reviewing it.
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    confirm_resources: bool,
    /// Reasoning effort supplied to the explicitly selected provider.
    #[arg(long, value_enum, hide = true, default_value_t = crate::advice::ReasoningEffort::Medium, requires = "infer", conflicts_with = "validate_artifact")]
    reasoning: crate::advice::ReasoningEffort,
    /// Provider connection timeout in seconds.
    #[arg(long, hide = true, default_value_t = 5, value_parser = clap::value_parser!(u64).range(1..=60), requires = "infer", conflicts_with = "validate_artifact")]
    connect_timeout_seconds: u64,
    /// Total model-load and generation timeout in seconds.
    #[arg(long, hide = true, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=3600), requires = "infer", conflicts_with = "validate_artifact")]
    timeout_seconds: u64,
    /// Maximum accepted provider response size in bytes.
    #[arg(long, hide = true, default_value_t = 1_048_576, value_parser = parse_max_response_bytes, requires = "infer", conflicts_with = "validate_artifact")]
    max_response_bytes: usize,
    /// Maximum generated output tokens requested from the provider.
    #[arg(long, hide = true, default_value_t = 2048, value_parser = parse_max_output_tokens, requires = "infer", conflicts_with = "validate_artifact")]
    max_output_tokens: usize,
    /// Total provider context window. Defaults to the input and output token budgets combined.
    #[arg(long, hide = true, value_parser = parse_runtime_context_tokens, requires = "infer", conflicts_with = "validate_artifact")]
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
    #[arg(long, hide = true, requires = "infer", conflicts_with = "validate_artifact")]
    mock_response: Option<PathBuf>,
    /// Render context as JSON or validated advice as Markdown/JSON. Context defaults to JSON.
    #[arg(long, value_enum)]
    format: Option<AdviceFormat>,
    /// Also write the selected rendering to this repo-relative or absolute path.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum AdviceFormat {
    Markdown,
    Json,
}
