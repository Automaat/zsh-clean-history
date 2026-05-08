use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use fs2::FileExt;
use tempfile::NamedTempFile;
use zsh_clean_history::cleaner::Removal;
use zsh_clean_history::{
    CleaningSettings, Paths, compact_exits_file, identify_removals, load_exit_codes,
    parse_history_file, write_log_entry,
};

#[derive(Parser)]
#[command(
    name = "zsh-clean-history",
    version,
    about = "Smart zsh history cleaner - removes typos and failed commands"
)]
struct Cli {
    #[arg(long, default_value_t = 0.8)]
    similarity: f64,
    #[arg(long, default_value_t = 3)]
    rare_threshold: usize,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, short)]
    quiet: bool,
    #[arg(long)]
    remove_rare: bool,
    #[arg(long)]
    no_log: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    Undo,
    RecordExit { timestamp: String, exit_code: i32 },
    Explain { command: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::from_home()?;

    match cli.cmd {
        Some(Cmd::Undo) => undo(&paths),
        Some(Cmd::RecordExit {
            timestamp,
            exit_code,
        }) => zsh_clean_history::exits::append_exit(&paths.exits, &timestamp, exit_code),
        Some(Cmd::Explain { ref command }) => explain(command, &cli, &paths),
        None => run_cleanup(&cli, &paths),
    }
}

fn run_cleanup(cli: &Cli, paths: &Paths) -> Result<()> {
    let settings = CleaningSettings {
        similarity_threshold: cli.similarity,
        rare_threshold: cli.rare_threshold,
        remove_rare: cli.remove_rare,
    };

    let _lock = LockedHistory::acquire(&paths.lock_file())?;

    let exit_codes = load_exit_codes(&paths.exits)?;
    let parsed = parse_history_file(&paths.history, &exit_codes)?;
    let removals = identify_removals(&parsed, &settings);
    let total_lines = parsed.entries.len();
    let drop_set = removals_set(&removals);

    if !cli.dry_run && !removals.is_empty() {
        let backup = paths.backup_for(&Local::now().format("%Y%m%d-%H%M%S").to_string());
        if paths.history.exists() {
            fs::copy(&paths.history, &backup)
                .with_context(|| format!("create backup at {}", backup.display()))?;
            prune_old_backups(&paths.history, 5)?;
        }
        write_history_atomically(&paths.history, &parsed.entries, &drop_set)?;
    }

    if !cli.dry_run {
        let keep_ts: HashSet<String> = parsed
            .entries
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| {
                if drop_set.contains(&idx) {
                    None
                } else {
                    e.timestamp.clone()
                }
            })
            .collect();
        compact_exits_file(&paths.exits, &keep_ts)?;
    }

    if !cli.no_log {
        if let Err(e) = write_log_entry(&paths.log, &settings, cli.dry_run, total_lines, &removals)
        {
            eprintln!("warning: could not write log: {e}");
        }
    }

    if !cli.quiet {
        if removals.is_empty() {
            println!("No commands to remove");
        } else {
            let action = if cli.dry_run {
                "Would remove"
            } else {
                "Removed"
            };
            println!("\n{action} {} lines:", removals.len());
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for r in &removals {
                *counts.entry(r.reason.as_str()).or_default() += 1;
            }
            let mut entries: Vec<_> = counts.into_iter().collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            for (reason, count) in entries {
                println!("  {reason}: {count}");
            }
        }
    }
    Ok(())
}

fn explain(command: &str, cli: &Cli, paths: &Paths) -> Result<()> {
    let settings = CleaningSettings {
        similarity_threshold: cli.similarity,
        rare_threshold: cli.rare_threshold,
        remove_rare: cli.remove_rare,
    };

    let exit_codes = load_exit_codes(&paths.exits)?;
    let parsed = parse_history_file(&paths.history, &exit_codes)?;
    let removals = identify_removals(&parsed, &settings);
    let removal_map: HashMap<usize, &Removal> =
        removals.iter().map(|r| (r.line, r)).collect();

    let success_count = parsed.successful_counts.get(command).copied().unwrap_or(0);
    let fail_count = parsed.failed_counts.get(command).copied().unwrap_or(0);
    let indices = parsed.cmd_to_lines.get(command).cloned().unwrap_or_default();

    if indices.is_empty() {
        println!("command not found in history: {command}");
        return Ok(());
    }

    println!("command:  {command}");
    println!("runs:     success={success_count} failed={fail_count}");
    println!("entries:  {}", indices.len());

    for &idx in &indices {
        print!("  [{idx}] ");
        if let Some(removal) = removal_map.get(&idx) {
            print!("REMOVE  reason: {}", removal.reason);
            if let Some(candidate) = extract_candidate(&removal.reason) {
                let sim = zsh_clean_history::similarity::ratio(command, &candidate);
                print!("  similarity: {sim:.2}");
            }
        } else {
            print!("KEEP");
        }
        println!();
    }

    Ok(())
}

fn extract_candidate(reason: &str) -> Option<String> {
    let start = reason.find('\'')?;
    let rest = &reason[start + 1..];
    let end = rest.rfind('\'')?;
    Some(rest[..end].to_string())
}

fn write_history_atomically(
    path: &Path,
    entries: &[zsh_clean_history::HistoryEntry],
    drop_set: &HashSet<usize>,
) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = NamedTempFile::new_in(parent)?;
    for (idx, entry) in entries.iter().enumerate() {
        if drop_set.contains(&idx) {
            continue;
        }
        tmp.write_all(entry.raw.as_bytes())?;
        tmp.write_all(b"\n")?;
    }
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    fs::File::open(parent)?.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn removals_set(removals: &[Removal]) -> HashSet<usize> {
    removals.iter().map(|r| r.line).collect()
}

fn prune_old_backups(history: &Path, keep: usize) -> Result<()> {
    let parent = history.parent().unwrap_or_else(|| Path::new("."));
    let prefix = format!(
        "{}.backup-",
        history
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(".zsh_history")
    );
    let mut backups: Vec<PathBuf> = fs::read_dir(parent)?
        .filter_map(|r| r.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect();
    backups.sort();
    if backups.len() > keep {
        let drop_count = backups.len() - keep;
        for path in backups.into_iter().take(drop_count) {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

fn undo(paths: &Paths) -> Result<()> {
    let _lock = LockedHistory::acquire(&paths.lock_file())?;
    let parent = paths
        .history
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let prefix = format!(
        "{}.backup-",
        paths
            .history
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(".zsh_history")
    );
    let mut backups: Vec<PathBuf> = fs::read_dir(&parent)?
        .filter_map(|r| r.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect();
    backups.sort();
    let latest = backups
        .last()
        .context("no backup files found to restore from")?
        .clone();
    fs::copy(&latest, &paths.history)?;
    println!(
        "Restored {} from {}",
        paths.history.display(),
        latest.display()
    );
    Ok(())
}

struct LockedHistory {
    file: fs::File,
}

impl LockedHistory {
    fn acquire(path: &Path) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("open {} for lock", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock {}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for LockedHistory {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}
