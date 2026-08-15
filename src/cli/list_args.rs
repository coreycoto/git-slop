#[derive(Debug, Args)]
struct ListArgs {
    #[command(subcommand)]
    command: ListCommand,
}

#[derive(Debug, Subcommand)]
enum ListCommand {
    /// List records that fail configured policy.
    PolicyFailures(FindingsListArgs),
    /// List maintenance candidates that warrant review.
    Interventions(FindingsListArgs),
    /// List advisory observations.
    Observations(FindingsListArgs),
    /// List advisory health findings.
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
    /// Report path; defaults to the latest durable or Git-private report.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Fail unless the report matches current repository state.
    #[arg(long)]
    require_current: bool,
    /// Maximum returned records.
    #[arg(long, default_value_t = 50)]
    top: usize,
    /// Output format.
    #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
    format: DisplayFormat,
    /// Use a wider terminal layout.
    #[arg(long)]
    wide: bool,
    /// Do not truncate terminal fields.
    #[arg(long)]
    no_truncate: bool,
}

#[derive(Debug, Args)]
struct FindingsListArgs {
    #[command(flatten)]
    output: ListOutputArgs,
    /// Match a finding path.
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
    /// Match review severity.
    #[arg(long)]
    severity: Option<String>,
    /// Match context/load band.
    #[arg(long)]
    context_band: Option<String>,
    /// Match maintenance-pressure band.
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
