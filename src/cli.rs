use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ccsearch",
    about = "Hybrid search CLI for Claude Code chat sessions",
    version,
    after_help = "Examples:\n  ccsearch \"authentication bug\"\n  ccsearch search \"refactor\" --days 7 --no-tui\n  ccsearch index --force\n  ccsearch list --days 30 --json"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Search sessions using hybrid BM25 + vector search
    Search(SearchArgs),

    /// Rebuild the search index
    Index(IndexArgs),

    /// List sessions without searching
    List(ListArgs),

    /// Show or edit configuration
    Config,
}

#[derive(Parser)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Only search sessions from the last N days
    #[arg(long, default_value_t = 30)]
    pub days: u32,

    /// Filter to a specific project path
    #[arg(long)]
    pub project: Option<String>,

    /// Maximum number of results
    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    /// Print results to stdout instead of TUI
    #[arg(long)]
    pub no_tui: bool,

    /// Output as JSON for scripting
    #[arg(long)]
    pub json: bool,

    /// BM25 weight in RRF fusion (default: 1.0)
    #[arg(long, default_value_t = 1.0)]
    pub bm25_weight: f64,

    /// Vector weight in RRF fusion (default: 1.0)
    #[arg(long, default_value_t = 1.0)]
    pub vec_weight: f64,
}

#[derive(Parser)]
pub struct IndexArgs {
    /// Only index sessions from the last N days
    #[arg(long)]
    pub days: Option<u32>,

    /// Reindex everything, ignoring staleness checks
    #[arg(long)]
    pub force: bool,

    /// Show per-session progress
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Only list sessions from the last N days
    #[arg(long, default_value_t = 30)]
    pub days: u32,

    /// Filter to a specific project path
    #[arg(long)]
    pub project: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}
