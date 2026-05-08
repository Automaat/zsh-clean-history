# zsh-clean-history

Smart zsh history cleanup plugin that automatically removes typos and failed commands based on similarity analysis.

## Features

- **Smart cleanup**: Removes failed commands similar to successful ones
- **Typo detection**: Finds rare commands similar to common variants
- **Exit code tracking**: Automatically captures command success/failure
- **Configurable**: Adjust similarity thresholds and behavior
- **Safe**: Creates backups before cleaning

## Installation

### Using zplug

```bash
# Add to ~/.zshrc
zplug "automaat/zsh-clean-history", from:github, at:main
```

### Using zinit

```bash
# Add to ~/.zshrc
zinit light automaat/zsh-clean-history
```

### Using oh-my-zsh

```bash
# Clone to custom plugins
git clone https://github.com/automaat/zsh-clean-history \
  ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-clean-history

# Add to plugins in ~/.zshrc
plugins=(... zsh-clean-history)
```

### Manual

```bash
# Clone
git clone https://github.com/automaat/zsh-clean-history ~/.zsh-clean-history

# Add to ~/.zshrc
source ~/.zsh-clean-history/zsh-clean-history.plugin.zsh
```

## Usage

### Commands

- `clean-history` - Run cleanup now
- `clean-history-stats` - Show stats without cleaning (dry run)
- `clean-history-info` - Show plugin configuration and commands
- `clean-history-log [N]` - Show summary of last N runs (default 5); pass `--full` for raw JSONL

### Configuration

Add before loading plugin in `~/.zshrc`:

```bash
# Auto-clean on shell exit (default: false)
ZSH_CLEAN_HISTORY_AUTO_CLEAN=true

# Similarity threshold 0-1 (default: 0.8)
ZSH_CLEAN_HISTORY_SIMILARITY=0.85

# Max occurrences to consider "rare" (default: 3)
ZSH_CLEAN_HISTORY_RARE_THRESHOLD=2

# Load plugin
zplug "automaat/zsh-clean-history", from:github
```

### Examples

```bash
# Run cleanup manually
clean-history

# Preview what would be removed
clean-history-stats

# See current configuration
clean-history-info

# Run with custom settings
clean-history --similarity 0.9 --rare-threshold 5

# Silent cleanup
clean-history --quiet
```

## How it works

1. **Exit code tracking**: Plugin captures exit codes for all commands
2. **Analysis**: Python script analyzes history to find:
   - Failed commands similar to successful ones (likely typos)
   - Rare commands similar to common ones (likely misspellings)
3. **Smart removal**: Removes problematic entries while preserving history

## Configuration options

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZSH_CLEAN_HISTORY_AUTO_CLEAN` | `false` | Auto-clean on shell exit |
| `ZSH_CLEAN_HISTORY_SIMILARITY` | `0.8` | Similarity threshold (0-1) |
| `ZSH_CLEAN_HISTORY_RARE_THRESHOLD` | `3` | Max count for "rare" commands |

### Command-line flags

- `--similarity FLOAT` - Override similarity threshold
- `--rare-threshold INT` - Override rare threshold
- `--dry-run` - Show what would be removed without changing history
- `--quiet` / `-q` - Minimal output
- `--no-log` - Skip writing this run to `~/.zsh_history_cleanup.log`

## Cleanup log

Every run (dry-run or real) appends one JSON line to `~/.zsh_history_cleanup.log`
(created with mode `0600` on first write, since entries embed command text):

```json
{"timestamp": "...", "dry_run": false, "settings": {...}, "total_lines": 858,
 "removed_count": 84, "reason_counts": {"Duplicate": 70, ...},
 "removals": [{"line": 56, "reason": "Failed similar to '...'", "command": "..."}, ...]}
```

`removals[].line` is the **0-based index** into the parsed `~/.zsh_history`
file (matches Python's `enumerate`, so the first command is `0`).

Use `clean-history-log` for a quick summary, `clean-history-log --full` for the raw
JSONL, or pipe through `jq` for analytics (`jq '.reason_counts | to_entries[]'`,
trend success/failure rates, audit which commands were dropped, etc.). Pass
`--no-log` to opt out for a single run.

## Requirements

- zsh
- Python 3.6+

## License

MIT
