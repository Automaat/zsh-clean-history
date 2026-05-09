use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use zsh_clean_history::allowlist::load_allowlist;
use zsh_clean_history::clean::{LockedHistory, run_cleanup};
use zsh_clean_history::cleaner::Removal;
use zsh_clean_history::{
    CleaningSettings, Paths, identify_removals, load_exit_codes, parse_history_file,
    write_log_entry,
};

include!("cli_definition.rs");

// Compile-time guard: keeps cli_definition.rs in sync with log.rs
const _: () = assert!(
    zsh_clean_history::DEFAULT_LOG_MAX_BYTES == DEFAULT_LOG_MAX_BYTES,
    "Update DEFAULT_LOG_MAX_BYTES in cli_definition.rs to match log.rs"
);

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
        None => do_cleanup(&cli, &paths),
    }
}

fn settings_from_cli(cli: &Cli) -> CleaningSettings {
    CleaningSettings {
        similarity_threshold: cli.similarity,
        rare_threshold: cli.rare_threshold,
        remove_rare: cli.remove_rare,
    }
}

fn do_cleanup(cli: &Cli, paths: &Paths) -> Result<()> {
    let settings = settings_from_cli(cli);
    let report = run_cleanup(paths, &settings, cli.dry_run)?;

    if !cli.no_log {
        if let Err(e) = write_log_entry(
            &paths.log,
            &settings,
            cli.dry_run,
            report.total_lines,
            &report.removals,
            cli.log_max_bytes,
        ) {
            eprintln!("warning: could not write log: {e}");
        }
    }

    if !cli.quiet {
        if report.removals.is_empty() {
            println!("No commands to remove");
        } else {
            let action = if cli.dry_run {
                "Would remove"
            } else {
                "Removed"
            };
            println!("\n{action} {} lines:", report.removals.len());
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for r in &report.removals {
                *counts.entry(&*r.reason).or_default() += 1;
            }
            let mut entries: Vec<_> = counts.into_iter().collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            for (reason, count) in &entries {
                println!("  {reason}: {count}");
                if cli.dry_run && cli.verbose {
                    for sample in report
                        .removals
                        .iter()
                        .filter(|r| &*r.reason == *reason)
                        .take(5)
                    {
                        println!("    {}", truncate_cmd(&sample.command, 70));
                    }
                }
            }
        }
    }
    Ok(())
}

fn explain(command: &str, cli: &Cli, paths: &Paths) -> Result<()> {
    let settings = settings_from_cli(cli);

    let exit_codes = load_exit_codes(&paths.exits)?;
    let parsed = parse_history_file(&paths.history, &exit_codes)?;
    let allowlist = load_allowlist(&paths.allowlist)?;
    let removals = identify_removals(&parsed, &settings, allowlist.as_ref());
    let removal_map: HashMap<usize, &Removal> = removals.iter().map(|r| (r.line, r)).collect();

    let success_count = parsed.successful_counts.get(command).copied().unwrap_or(0);
    let fail_count = parsed.failed_counts.get(command).copied().unwrap_or(0);
    let indices = parsed
        .cmd_to_lines
        .get(command)
        .cloned()
        .unwrap_or_default();

    if indices.is_empty() {
        anyhow::bail!("command not found in history: {command}");
    }

    println!("command:  {command}");
    println!("runs:     success={success_count} failed={fail_count}");
    println!("entries:  {}", indices.len());

    for &idx in &indices {
        print!("  [{idx}] ");
        if let Some(removal) = removal_map.get(&idx) {
            print!("REMOVE  reason: {}", removal.reason);
        } else {
            print!("KEEP");
        }
        println!();
    }

    Ok(())
}

fn truncate_cmd(cmd: &str, max_chars: usize) -> String {
    if max_chars < 3 {
        return cmd.to_string();
    }
    let chars: Vec<char> = cmd.chars().collect();
    if chars.len() <= max_chars {
        cmd.to_string()
    } else {
        format!("{}...", chars[..max_chars - 3].iter().collect::<String>())
    }
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
    let mut backups: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&parent)? {
        match entry {
            Ok(e) => {
                let path = e.path();
                if path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|n| n.starts_with(&prefix))
                    .unwrap_or(false)
                {
                    backups.push(path);
                }
            }
            Err(e) => eprintln!("warning: could not read backup entry: {e}"),
        }
    }
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
