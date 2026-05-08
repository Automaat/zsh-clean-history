
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'zsh-clean-history' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'zsh-clean-history'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'zsh-clean-history' {
            [CompletionResult]::new('--similarity', '--similarity', [CompletionResultType]::ParameterName, 'similarity')
            [CompletionResult]::new('--rare-threshold', '--rare-threshold', [CompletionResultType]::ParameterName, 'rare-threshold')
            [CompletionResult]::new('--log-max-bytes', '--log-max-bytes', [CompletionResultType]::ParameterName, 'log-max-bytes')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'dry-run')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'q')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'quiet')
            [CompletionResult]::new('--remove-rare', '--remove-rare', [CompletionResultType]::ParameterName, 'remove-rare')
            [CompletionResult]::new('--no-log', '--no-log', [CompletionResultType]::ParameterName, 'no-log')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('undo', 'undo', [CompletionResultType]::ParameterValue, 'undo')
            [CompletionResult]::new('record-exit', 'record-exit', [CompletionResultType]::ParameterValue, 'record-exit')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'explain')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'zsh-clean-history;undo' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'zsh-clean-history;record-exit' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'zsh-clean-history;explain' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'zsh-clean-history;help' {
            [CompletionResult]::new('undo', 'undo', [CompletionResultType]::ParameterValue, 'undo')
            [CompletionResult]::new('record-exit', 'record-exit', [CompletionResultType]::ParameterValue, 'record-exit')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'explain')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'zsh-clean-history;help;undo' {
            break
        }
        'zsh-clean-history;help;record-exit' {
            break
        }
        'zsh-clean-history;help;explain' {
            break
        }
        'zsh-clean-history;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
