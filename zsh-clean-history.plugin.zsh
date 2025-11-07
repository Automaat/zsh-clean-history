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
    local cmd="${1%%$'\n'}"
    local timestamp=$EPOCHSECONDS

    # Append to exit codes file
    echo "${timestamp}:${exit_code}" >> "$_zsh_clean_history_exit_file"
    _zsh_clean_history_exits[$timestamp]=$exit_code

    # Let default history mechanism work
    return 0
}

# Load existing exit codes
_zsh_clean_history_load_exits

# Hook into zsh
autoload -Uz add-zsh-hook
add-zsh-hook zshaddhistory _zsh_clean_history_save_exit

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

# Auto-clean on shell exit if enabled
if [[ "$ZSH_CLEAN_HISTORY_AUTO_CLEAN" == "true" ]]; then
    _zsh_clean_history_exit() {
        clean-history --quiet
    }
    add-zsh-hook zshexit _zsh_clean_history_exit
fi
