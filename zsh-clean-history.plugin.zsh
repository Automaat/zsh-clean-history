#!/usr/bin/env zsh
# zsh-clean-history: Smart history cleanup plugin
# Removes typos and failed commands based on similarity analysis

# Get plugin directory
ZSH_CLEAN_HISTORY_DIR="${0:A:h}"
ZSH_CLEAN_HISTORY_SCRIPT="${ZSH_CLEAN_HISTORY_DIR}/clean_history.py"

# Configuration (can be overridden in .zshrc before loading plugin)
ZSH_CLEAN_HISTORY_AUTO_CLEAN=${ZSH_CLEAN_HISTORY_AUTO_CLEAN:-false}  # Auto-clean on shell exit
ZSH_CLEAN_HISTORY_SIMILARITY=${ZSH_CLEAN_HISTORY_SIMILARITY:-0.8}
ZSH_CLEAN_HISTORY_RARE_THRESHOLD=${ZSH_CLEAN_HISTORY_RARE_THRESHOLD:-3}

# Enable extended history with exit codes
setopt EXTENDED_HISTORY
setopt INC_APPEND_HISTORY

# Store exit codes in separate file
typeset -g _zsh_clean_history_exit_file="${HOME}/.zsh_history_exits"
typeset -gA _zsh_clean_history_exits

_zsh_clean_history_load_exits() {
    [[ -f "$_zsh_clean_history_exit_file" ]] || return
    local line timestamp cmd exit_code
    while IFS=: read -r timestamp exit_code; do
        _zsh_clean_history_exits[$timestamp]=$exit_code
    done < "$_zsh_clean_history_exit_file"
}

_zsh_clean_history_save_exit() {
    local exit_code=$?

    # Get the last history entry's timestamp from the history file
    # History format: : timestamp:duration;command
    local last_line="$(tail -1 "$HISTFILE" 2>/dev/null)"
    local timestamp

    # Extract timestamp from history line using regex
    if [[ "$last_line" =~ "^: ([0-9]+):[0-9]+;" ]]; then
        timestamp="${match[1]}"
    else
        # Fallback to current time if we can't parse the timestamp
        timestamp=$EPOCHSECONDS
    fi

    # Append to exit codes file
    echo "${timestamp}:${exit_code}" >> "$_zsh_clean_history_exit_file"
    _zsh_clean_history_exits[$timestamp]=$exit_code
}

# Load existing exit codes
_zsh_clean_history_load_exits

# Hook into zsh - use precmd to capture exit code AFTER command runs
autoload -Uz add-zsh-hook
add-zsh-hook precmd _zsh_clean_history_save_exit

# Command to run cleanup manually
clean-history() {
    if [[ ! -f "$ZSH_CLEAN_HISTORY_SCRIPT" ]]; then
        echo "Error: Cleanup script not found at $ZSH_CLEAN_HISTORY_SCRIPT"
        return 1
    fi

    python3 "$ZSH_CLEAN_HISTORY_SCRIPT" \
        --similarity "$ZSH_CLEAN_HISTORY_SIMILARITY" \
        --rare-threshold "$ZSH_CLEAN_HISTORY_RARE_THRESHOLD" \
        "$@"
}

# Command to show plugin info
clean-history-info() {
    echo "zsh-clean-history plugin"
    echo ""
    echo "Configuration:"
    echo "  Auto-clean on exit: $ZSH_CLEAN_HISTORY_AUTO_CLEAN"
    echo "  Similarity threshold: $ZSH_CLEAN_HISTORY_SIMILARITY"
    echo "  Rare threshold: $ZSH_CLEAN_HISTORY_RARE_THRESHOLD"
    echo ""
    echo "Commands:"
    echo "  clean-history       - Run cleanup now"
    echo "  clean-history-info  - Show this info"
    echo "  clean-history-stats - Show history statistics"
}

# Command to show stats without cleaning
clean-history-stats() {
    if [[ ! -f "$ZSH_CLEAN_HISTORY_SCRIPT" ]]; then
        echo "Error: Cleanup script not found at $ZSH_CLEAN_HISTORY_SCRIPT"
        return 1
    fi

    python3 "$ZSH_CLEAN_HISTORY_SCRIPT" --dry-run
}

# Command to inspect recent cleanup runs (default: last 5, --full for full JSON)
clean-history-log() {
    local logfile="${HOME}/.zsh_history_cleanup.log"
    local n=5 full=false

    while (( $# )); do
        case "$1" in
            --full) full=true; shift ;;
            -n) n="$2"; shift 2 ;;
            *) n="$1"; shift ;;
        esac
    done

    if [[ ! -f "$logfile" ]]; then
        echo "No cleanup log yet at $logfile"
        echo "Run 'clean-history' or 'clean-history-stats' to generate entries."
        return 1
    fi

    if [[ "$full" == true ]]; then
        tail -n "$n" "$logfile"
        return
    fi

    tail -n "$n" "$logfile" | python3 -c '
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        e = json.loads(line)
    except ValueError:
        print(line)
        continue
    mode = "DRY-RUN" if e.get("dry_run") else "APPLIED"
    ts = e["timestamp"]
    rm = e["removed_count"]
    tot = e["total_lines"]
    sim = e["settings"]["similarity"]
    rare = e["settings"]["rare_threshold"]
    print(f"{ts}  {mode}  removed={rm}/{tot}  sim={sim} rare<={rare}")
    for reason, count in sorted(e.get("reason_counts", {}).items(), key=lambda x: -x[1]):
        print(f"    {count:4}  {reason}")
    samples = [r for r in e.get("removals", []) if r["reason"] != "Duplicate"][:5]
    if samples:
        print("    samples:")
        for r in samples:
            cmd = r["command"]
            if len(cmd) > 70:
                cmd = cmd[:67] + "..."
            reason = r["reason"]
            print(f"      [{reason}] {cmd}")
    print()
'
}

# Auto-clean on shell exit if enabled
if [[ "$ZSH_CLEAN_HISTORY_AUTO_CLEAN" == "true" ]]; then
    _zsh_clean_history_exit() {
        clean-history --quiet
    }
    add-zsh-hook zshexit _zsh_clean_history_exit
fi
