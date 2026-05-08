use strsim::damerau_levenshtein;

pub fn ratio(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = damerau_levenshtein(a, b);
    1.0 - (dist as f64 / max_len as f64)
}

pub fn base_command(cmd: &str) -> &str {
    cmd.split_whitespace().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_returns_one() {
        assert_eq!(ratio("git status", "git status"), 1.0);
    }

    #[test]
    fn empty_strings_return_one() {
        assert_eq!(ratio("", ""), 1.0);
    }

    #[test]
    fn typo_above_threshold() {
        assert!(ratio("git statsu", "git status") > 0.8);
    }

    #[test]
    fn unrelated_below_threshold() {
        assert!(ratio("ls -la", "kubectl get pods") < 0.5);
    }

    #[test]
    fn base_command_extracts_first_word() {
        assert_eq!(base_command("git status -s"), "git");
        assert_eq!(base_command("ls"), "ls");
        assert_eq!(base_command(""), "");
        assert_eq!(base_command("   spaces   first"), "spaces");
    }
}
