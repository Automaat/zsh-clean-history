_zsh-clean-history() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="zsh__clean__history"
                ;;
            zsh__clean__history,explain)
                cmd="zsh__clean__history__subcmd__explain"
                ;;
            zsh__clean__history,help)
                cmd="zsh__clean__history__subcmd__help"
                ;;
            zsh__clean__history,record-exit)
                cmd="zsh__clean__history__subcmd__record__subcmd__exit"
                ;;
            zsh__clean__history,undo)
                cmd="zsh__clean__history__subcmd__undo"
                ;;
            zsh__clean__history__subcmd__help,explain)
                cmd="zsh__clean__history__subcmd__help__subcmd__explain"
                ;;
            zsh__clean__history__subcmd__help,help)
                cmd="zsh__clean__history__subcmd__help__subcmd__help"
                ;;
            zsh__clean__history__subcmd__help,record-exit)
                cmd="zsh__clean__history__subcmd__help__subcmd__record__subcmd__exit"
                ;;
            zsh__clean__history__subcmd__help,undo)
                cmd="zsh__clean__history__subcmd__help__subcmd__undo"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        zsh__clean__history)
            opts="-q -h -V --similarity --rare-threshold --dry-run --quiet --remove-rare --no-log --log-max-bytes --help --version undo record-exit explain help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --similarity)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --rare-threshold)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --log-max-bytes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__explain)
            opts="-h --help <COMMAND>"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__help)
            opts="undo record-exit explain help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__help__subcmd__explain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__help__subcmd__record__subcmd__exit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__help__subcmd__undo)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__record__subcmd__exit)
            opts="-h --help <TIMESTAMP> <EXIT_CODE>"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        zsh__subcmd__clean__subcmd__history__subcmd__undo)
            opts="-h --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _zsh-clean-history -o nosort -o bashdefault -o default zsh-clean-history
else
    complete -F _zsh-clean-history -o bashdefault -o default zsh-clean-history
fi
