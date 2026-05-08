#!/usr/bin/env zsh
# zsh-clean-history: smart history cleanup plugin (Rust backend)

ZSH_CLEAN_HISTORY_DIR="${0:A:h}"
: ${ZSH_CLEAN_HISTORY_BIN:=zsh-clean-history}
: ${ZSH_CLEAN_HISTORY_AUTO_CLEAN:=false}
: ${ZSH_CLEAN_HISTORY_SIMILARITY:=0.8}
: ${ZSH_CLEAN_HISTORY_RARE_THRESHOLD:=3}

setopt EXTENDED_HISTORY
setopt INC_APPEND_HISTORY

_zsh_clean_history_resolve_bin() {
    if (( $+commands[$ZSH_CLEAN_HISTORY_BIN] )); then
        echo "$commands[$ZSH_CLEAN_HISTORY_BIN]"
        return
    fi
    local local_bin="${ZSH_CLEAN_HISTORY_DIR}/target/release/zsh-clean-history"
    if [[ -x "$local_bin" ]]; then
        echo "$local_bin"
        return
    fi
    local debug_bin="${ZSH_CLEAN_HISTORY_DIR}/target/debug/zsh-clean-history"
    if [[ -x "$debug_bin" ]]; then
        echo "$debug_bin"
        return
    fi
    return 1
}

zmodload zsh/datetime 2>/dev/null
typeset -g _zsh_clean_history_exit_file="${HOME}/.zsh_history_exits"
typeset -g _zsh_clean_history_pending_ts=0
typeset -gi _zsh_clean_history_pending_histcmd=0
typeset -gi _zsh_clean_history_recorded_histcmd=0

# Ensure exit file exists with 0600 permissions.  Run in a subshell so the
# umask change does not affect the calling shell.
if [[ ! -f "$_zsh_clean_history_exit_file" ]]; then
    (umask 0177 && : >>! "$_zsh_clean_history_exit_file") 2>/dev/null
else
    chmod 0600 "$_zsh_clean_history_exit_file" 2>/dev/null
fi

# preexec captures EPOCHREALTIME at command start, which matches the timestamp
# zsh writes into HISTFILE for EXTENDED_HISTORY entries. Without this, long-
# running commands (e.g. `sleep 60`) would record an end-time timestamp that
# never matches the history entry. Microsecond precision reduces file-level
# duplicate lines for commands run within the same second.
_zsh_clean_history_record_start() {
    _zsh_clean_history_pending_ts=$EPOCHREALTIME
    _zsh_clean_history_pending_histcmd=$HISTCMD
}

_zsh_clean_history_save_exit() {
    local code=$?
    # No command pending (initial prompt before first command runs)
    (( _zsh_clean_history_pending_histcmd == 0 )) && return 0
    # precmd can fire without a new command (bare Enter, line-edit interrupt);
    # skip if we already recorded this HISTCMD.
    (( _zsh_clean_history_pending_histcmd == _zsh_clean_history_recorded_histcmd )) && return 0
    print -r -- "${_zsh_clean_history_pending_ts}:${code}" >>! "$_zsh_clean_history_exit_file"
    _zsh_clean_history_recorded_histcmd=$_zsh_clean_history_pending_histcmd
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec _zsh_clean_history_record_start
add-zsh-hook precmd _zsh_clean_history_save_exit

clean-history() {
    local bin
    bin="$(_zsh_clean_history_resolve_bin)" || {
        echo "zsh-clean-history: binary not found. Run 'cargo install --path ${ZSH_CLEAN_HISTORY_DIR}' or place 'zsh-clean-history' on PATH." >&2
        return 1
    }
    "$bin" \
        --similarity "$ZSH_CLEAN_HISTORY_SIMILARITY" \
        --rare-threshold "$ZSH_CLEAN_HISTORY_RARE_THRESHOLD" \
        "$@"
}

clean-history-stats() {
    clean-history --dry-run "$@"
}

clean-history-undo() {
    local bin
    bin="$(_zsh_clean_history_resolve_bin)" || return 1
    "$bin" undo "$@"
}

clean-history-info() {
    echo "zsh-clean-history (Rust)"
    echo
    echo "Configuration:"
    echo "  Auto-clean on exit:   $ZSH_CLEAN_HISTORY_AUTO_CLEAN"
    echo "  Similarity threshold: $ZSH_CLEAN_HISTORY_SIMILARITY"
    echo "  Rare threshold:       $ZSH_CLEAN_HISTORY_RARE_THRESHOLD"
    local bin
    if bin="$(_zsh_clean_history_resolve_bin)"; then
        echo "  Binary:               $bin"
    else
        echo "  Binary:               (not found on PATH)"
    fi
    echo
    echo "Commands:"
    echo "  clean-history       - Run cleanup now"
    echo "  clean-history-stats - Dry run"
    echo "  clean-history-undo  - Restore newest backup"
    echo "  clean-history-info  - Show this info"
    echo "  clean-history-log   - Show last cleanup runs"
}

clean-history-log() {
    local logfile="${HOME}/.zsh_history_cleanup.log"
    local n=5 full=false

    while (( $# )); do
        case "$1" in
            --full) full=true; shift ;;
            -n)
                if [[ -z "${2:-}" || ! "$2" =~ ^[0-9]+$ ]]; then
                    echo "clean-history-log: -n requires an integer argument" >&2
                    return 2
                fi
                n="$2"; shift 2 ;;
            *)
                if [[ ! "$1" =~ ^[0-9]+$ ]]; then
                    echo "clean-history-log: invalid argument '$1'" >&2
                    return 2
                fi
                n="$1"; shift ;;
        esac
    done

    if [[ ! -f "$logfile" ]]; then
        echo "No cleanup log yet at $logfile"
        return 1
    fi

    if [[ "$full" == true ]]; then
        tail -n "$n" "$logfile"
        return
    fi

    if (( $+commands[jq] )); then
        tail -n "$n" "$logfile" | jq -r '
          (if .dry_run then "DRY-RUN" else "APPLIED" end) as $mode
          | "\(.timestamp)  \($mode)  removed=\(.removed_count)/\(.total_lines)  sim=\(.settings.similarity) rare<=\(.settings.rare_threshold)",
            (.reason_counts | to_entries | sort_by(-.value) | .[] | "    \(.value)  \(.key)"),
            ((.removals // []) | map(select(.reason != "Duplicate"))[:5] | if length > 0 then "    samples:" else empty end),
            ((.removals // []) | map(select(.reason != "Duplicate"))[:5] | .[] | "      [\(.reason)] \(.command[0:70])"),
            ""
        '
    else
        tail -n "$n" "$logfile"
    fi
}

if [[ "$ZSH_CLEAN_HISTORY_AUTO_CLEAN" == "true" ]]; then
    _zsh_clean_history_exit() {
        {
            local out rc
            out=$(clean-history --quiet 2>&1)
            rc=$?
            if (( rc != 0 )); then
                local ts
                strftime -s ts '%Y-%m-%dT%H:%M:%SZ' $EPOCHSECONDS
                printf '[%s] ERROR exit=%d %s\n' "$ts" "$rc" "$out" \
                    >> "${HOME}/.zsh_history_cleanup.log"
            fi
        } </dev/null &!
    }
    add-zsh-hook zshexit _zsh_clean_history_exit
fi
