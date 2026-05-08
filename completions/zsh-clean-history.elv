
use builtin;
use str;

set edit:completion:arg-completer[zsh-clean-history] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'zsh-clean-history'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'zsh-clean-history'= {
            cand --similarity 'similarity'
            cand --rare-threshold 'rare-threshold'
            cand --log-max-bytes 'log-max-bytes'
            cand --dry-run 'dry-run'
            cand -q 'q'
            cand --quiet 'quiet'
            cand --remove-rare 'remove-rare'
            cand --no-log 'no-log'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand undo 'undo'
            cand record-exit 'record-exit'
            cand explain 'explain'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'zsh-clean-history;undo'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'zsh-clean-history;record-exit'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'zsh-clean-history;explain'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'zsh-clean-history;help'= {
            cand undo 'undo'
            cand record-exit 'record-exit'
            cand explain 'explain'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'zsh-clean-history;help;undo'= {
        }
        &'zsh-clean-history;help;record-exit'= {
        }
        &'zsh-clean-history;help;explain'= {
        }
        &'zsh-clean-history;help;help'= {
        }
    ]
    $completions[$command]
}
