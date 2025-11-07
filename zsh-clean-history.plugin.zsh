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

# Capture exit codes in history
_zsh_clean_history_last_exit=0

_zsh_clean_history_precmd() {
    _zsh_clean_history_last_exit=$?
}

_zsh_clean_history_addhistory() {
    print -sr -- "${1%%$'\n'}###EXIT:${_zsh_clean_history_last_exit}"
    return 1  # Prevent default history add
}

# Hook into zsh
autoload -Uz add-zsh-hook
add-zsh-hook precmd _zsh_clean_history_precmd
add-zsh-hook zshaddhistory _zsh_clean_history_addhistory

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
