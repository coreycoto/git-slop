#[derive(Debug, Args)]
struct CacheArgs {
    /// Mutable state directory. Defaults to the same active root as find.
    #[arg(long, value_name = "PATH", global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: CacheCommand,
}

#[derive(Debug, Subcommand)]
enum CacheCommand {
    /// Show packed-cache location, entry count, and logical size.
    Status {
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
    /// Preview or apply bounded packed-cache retention.
    Prune {
        /// Maximum entries to retain.
        #[arg(long, default_value_t = 10_000)]
        max_entries: usize,
        /// Maximum logical payload bytes to retain.
        #[arg(long, default_value_t = 536_870_912)]
        max_bytes: u64,
        /// Explicitly request preview behavior (preview is already the default).
        #[arg(long, conflicts_with = "yes")]
        dry_run: bool,
        /// Apply the selected removals. Without this flag the command is read-only.
        #[arg(long, conflicts_with = "dry_run")]
        yes: bool,
        /// Reclaim free database pages after pruning.
        #[arg(long)]
        compact: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = DisplayFormat::Text)]
        format: DisplayFormat,
    },
}
