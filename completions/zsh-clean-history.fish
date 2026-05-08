# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_zsh_clean_history_global_optspecs
	string join \n similarity= rare-threshold= dry-run q/quiet remove-rare no-log log-max-bytes= h/help V/version
end

function __fish_zsh_clean_history_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_zsh_clean_history_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_zsh_clean_history_using_subcommand
	set -l cmd (__fish_zsh_clean_history_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -l similarity -r
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -l rare-threshold -r
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -l log-max-bytes -r
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -l dry-run
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -s q -l quiet
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -l remove-rare
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -l no-log
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -s h -l help -d 'Print help'
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -s V -l version -d 'Print version'
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -f -a "undo"
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -f -a "record-exit"
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -f -a "explain"
complete -c zsh-clean-history -n "__fish_zsh_clean_history_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c zsh-clean-history -n "__fish_zsh_clean_history_using_subcommand undo" -s h -l help -d 'Print help'
complete -c zsh-clean-history -n "__fish_zsh_clean_history_using_subcommand record-exit" -s h -l help -d 'Print help'
complete -c zsh-clean-history -n "__fish_zsh_clean_history_using_subcommand explain" -s h -l help -d 'Print help'
complete -c zsh-clean-history -n "__fish_zsh_clean_history_using_subcommand help; and not __fish_seen_subcommand_from undo record-exit explain help" -f -a "undo"
complete -c zsh-clean-history -n "__fish_zsh_clean_history_using_subcommand help; and not __fish_seen_subcommand_from undo record-exit explain help" -f -a "record-exit"
complete -c zsh-clean-history -n "__fish_zsh_clean_history_using_subcommand help; and not __fish_seen_subcommand_from undo record-exit explain help" -f -a "explain"
complete -c zsh-clean-history -n "__fish_zsh_clean_history_using_subcommand help; and not __fish_seen_subcommand_from undo record-exit explain help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
