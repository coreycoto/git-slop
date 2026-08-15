#[derive(Debug, Args)]
struct ListArgs {
    #[command(subcommand)]
    command: ListCommand,
}

#[derive(Debug, Subcommand)]
enum ListCommand {
    /// List policy-enforced failures from repository health.
    PolicyFailures(FindingsListArgs),
    /// List bounded maintenance candidates that warrant review.
    Interventions(FindingsListArgs),
    /// List observation-only signals that do not request intervention.
    Observations(FindingsListArgs),
    /// List advisory repository-health findings.
    HealthFindings(FindingsListArgs),
    /// Deprecated compatibility name for `health-findings`.
    Findings(FindingsListArgs),
    /// List evidence-backed relationships between paths.
    Relationships(RelationshipsListArgs),
    /// List structural or consolidation clusters.
    Clusters(ClustersListArgs),
    /// List aggregate analysis-profile totals.
    Profiles(ProfilesListArgs),
}

#[derive(Debug, Args)]
struct ListOutputArgs {
    /// Report path. Defaults to the durable latest report, then the Git-private first-run report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Fail when the report does not match current HEAD, worktree, config, scope, or analyzer.
    #[arg(long)]
    require_current: bool,
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
struct FindingsListArgs {
    #[command(flatten)]
    output: ListOutputArgs,
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
    /// Match delivery severity (error, warning, or notice).
    #[arg(long)]
    severity: Option<String>,
    /// Match detector context band independently of severity.
    #[arg(long)]
    context_band: Option<String>,
    /// Match detector maintenance-pressure band independently of severity.
    #[arg(long)]
    slop_band: Option<String>,
}

#[derive(Debug, Args)]
struct RelationshipsListArgs {
    #[command(flatten)]
    output: ListOutputArgs,
    /// Match a relationship endpoint.
    #[arg(long)]
    path: Option<String>,
    /// Match an endpoint analysis profile.
    #[arg(long)]
    profile: Option<String>,
    /// Match an endpoint file language.
    #[arg(long)]
    language: Option<String>,
    /// Match an endpoint file classification.
    #[arg(long, visible_alias = "class")]
    classification: Option<String>,
}

#[derive(Debug, Args)]
struct ClustersListArgs {
    #[command(flatten)]
    output: ListOutputArgs,
    /// Match a cluster member path.
    #[arg(long)]
    path: Option<String>,
    /// Match a member analysis profile.
    #[arg(long)]
    profile: Option<String>,
    /// Match a member file language.
    #[arg(long)]
    language: Option<String>,
    /// Match a member file classification.
    #[arg(long, visible_alias = "class")]
    classification: Option<String>,
}

#[derive(Debug, Args)]
struct ProfilesListArgs {
    #[command(flatten)]
    output: ListOutputArgs,
    /// Match an analysis profile.
    #[arg(long)]
    profile: Option<String>,
}
