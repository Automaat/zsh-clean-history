# zsh-clean-history

Smart zsh history cleanup. Removes typos, failed commands, and duplicates from `~/.zsh_history` based on similarity analysis. Written in Rust.

## Features

- Removes failed commands that are typos of successful ones (Damerau-Levenshtein distance — handles transpositions natively)
- Removes cross-base typos — e.g. `gti status` when `git` has ≥ 20 uses (base count ≤ 2, candidate count ≥ 20, DL distance 1)
- Removes rare command variants similar to common ones
- Deduplicates while keeping the most-recent occurrence (Ctrl-R-friendly)
- Atomic writes with file locking — safe under concurrent shells
- Timestamped backup rotation, plus `clean-history-undo` to restore the latest
- JSONL run log at `~/.zsh_history_cleanup.log`
- Multi-line history entries handled correctly

## Install

### Build & install the binary

```bash
git clone https://github.com/automaat/zsh-clean-history ~/.zsh-clean-history
cd ~/.zsh-clean-history
cargo install --path . --locked
```

This puts `zsh-clean-history` on your `PATH` (via `~/.cargo/bin`).

### Load the plugin

#### oh-my-zsh

```bash
ln -s ~/.zsh-clean-history ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-clean-history
# add to plugins=(... zsh-clean-history) in ~/.zshrc
```

#### Manual

```bash
echo 'source ~/.zsh-clean-history/zsh-clean-history.plugin.zsh' >> ~/.zshrc
```

The plugin auto-finds the binary on `PATH`, or falls back to `target/release/zsh-clean-history` inside the plugin dir.

## Commands

| Command | Description |
|---|---|
| `clean-history` | Run cleanup |
| `clean-history-stats` | Dry run (no writes) |
| `clean-history-undo` | Restore the most recent backup |
| `clean-history-info` | Show config |
| `clean-history-log [N]` | Summarize last N runs (`--full` for raw JSONL) |

## Configuration

```bash
# Auto-clean on shell exit (default: false); runs in background, non-blocking
# Failures are logged to ~/.zsh_history_cleanup.log
ZSH_CLEAN_HISTORY_AUTO_CLEAN=true

# Similarity threshold 0..1 (default: 0.8)
ZSH_CLEAN_HISTORY_SIMILARITY=0.85

# Max weighted score considered "rare" (default: 3.0).
# Uses time-decay weights: 1.0 for <7d, 0.5 for 8-30d, 0.1 for 31d+.
ZSH_CLEAN_HISTORY_RARE_THRESHOLD=2.0

# Override binary location (default: search PATH, then plugin's target/)
ZSH_CLEAN_HISTORY_BIN=/path/to/zsh-clean-history
```

## CLI flags

```
zsh-clean-history [--similarity F] [--rare-threshold F]
                  [--dry-run] [--quiet] [--remove-rare] [--no-log]
zsh-clean-history undo
zsh-clean-history record-exit <timestamp> <code>
```

## How it works

1. Plugin's `precmd` hook appends `<timestamp>:<exit-code>` to `~/.zsh_history_exits` for every command.
2. On cleanup, `zsh-clean-history`:
   - Locks `~/.zsh_history` (`flock`),
   - Parses entries (multi-line aware) and joins with exit codes,
   - Identifies removals via four strategies:
     - **Duplicate** — keep newest occurrence,
     - **Failed prefix / similar** — failed commands that are typos of successful ones,
     - **Cross-base typo** — base command with total count ≤ 2 within DL distance 1 of a base with count ≥ 20,
     - **Rare variant** *(opt-in via `--remove-rare`)* — uncommon spellings of common commands,
   - Writes a timestamped backup,
   - Writes the cleaned history atomically (`tempfile` + `rename`),
   - Compacts `~/.zsh_history_exits` to drop entries for now-deleted commands.

## Cleanup log

Each run appends one JSON line to `~/.zsh_history_cleanup.log` (chmod `0600`):

```json
{"timestamp":"2026-05-08T11:32:00Z","dry_run":false,"settings":{"similarity":0.8,"rare_threshold":3,"remove_rare":false},"total_lines":858,"removed_count":84,"reason_counts":{"Duplicate":70,"Failed similar to 'git status'":2},"removals":[{"line":56,"reason":"Failed similar to 'git status'","command":"git statsu"}]}
```

`removals[].line` is the 0-based index in the parsed history.

`clean-history-log` summarises recent runs; `clean-history-log --full` dumps raw JSONL. Pipe through `jq` for analytics.

## Development

```bash
mise run install   # cargo install --path .
mise run test      # cargo test --all-targets
mise run check     # fmt-check + clippy + zsh -n + actionlint
```

## Requirements

- zsh
- Rust 1.85+ to build

## License

MIT
