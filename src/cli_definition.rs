// Shared CLI definition — included by both src/main.rs and build.rs via include!().
// Keep this file free of dependencies on the zsh_clean_history lib so build.rs
// can include it without a circular dependency.

const DEFAULT_LOG_MAX_BYTES: u64 = 1_048_576;

#[derive(Parser)]
#[command(
    name = "zsh-clean-history",
    version,
    about = "Smart zsh history cleaner - removes typos and failed commands"
)]
struct Cli {
    #[arg(long, default_value_t = 0.8)]
    similarity: f64,
    #[arg(long, default_value_t = 3.0)]
    rare_threshold: f64,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, short = 'v')]
    verbose: bool,
    #[arg(long, short)]
    quiet: bool,
    #[arg(long)]
    remove_rare: bool,
    #[arg(long)]
    no_log: bool,
    #[arg(long, default_value_t = DEFAULT_LOG_MAX_BYTES)]
    log_max_bytes: u64,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    Undo,
    RecordExit { timestamp: String, exit_code: i32 },
    Explain { command: String },
}
