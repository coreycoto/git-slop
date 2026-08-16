#[derive(Debug, Args)]
struct PolicyArgs {
    #[command(subcommand)]
    command: PolicyCommand,
}

#[derive(Debug, Subcommand)]
enum PolicyCommand {
    /// Create a minimal data-only policy pack.
    Init {
        /// Empty directory to populate. Relative paths resolve from the repository root.
        directory: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
    /// Validate a local directory, installed pack ID, or the built-in core pack.
    Validate {
        /// Policy-pack directory or installed pack ID.
        target: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
    /// Run static golden cases for a local or installed data-only pack.
    Test {
        /// Policy-pack directory or installed pack ID.
        target: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
    /// Explicitly copy a validated local pack into the user policy cache.
    Install {
        /// Local policy-pack directory. Network acquisition is not implicit in v1.
        source: PathBuf,
        /// Add this pack to .slop/policies.yaml after installation.
        #[arg(long)]
        select: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
    /// Resolve selected packs and write .slop/policy-lock.json.
    Lock {
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
    /// List the built-in and user-installed policy packs.
    List {
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
    /// Inspect a complete pack or one stable rule ID.
    Show {
        /// Installed pack ID, rule ID, core, or local pack directory.
        target: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
    /// Remove an installed third-party pack from the user cache.
    Remove {
        /// Installed policy-pack ID.
        pack_id: String,
        /// Remove the pack from .slop/policies.yaml and invalidate its lock first.
        #[arg(long)]
        unselect: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = PolicyFormat::Text)]
        format: PolicyFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PolicyFormat {
    Text,
    Json,
}
