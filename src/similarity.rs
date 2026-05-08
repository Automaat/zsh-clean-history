use strsim::damerau_levenshtein;

const FALLBACK_THRESHOLD: f64 = 0.95;

// Intentionally crate-private: the public API exposes `command_similar`;
// raw ratio values are an implementation detail not guaranteed to be stable.
pub(crate) fn ratio(a: &str, b: &str) -> f64 {
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

/// Split a command into its first two whitespace-delimited words and the rest.
fn command_split(cmd: &str) -> (String, String) {
    let mut tokens = cmd.split_whitespace();
    let w0 = tokens.next().unwrap_or("");
    let w1 = tokens.next().unwrap_or("");
    let rest: Vec<&str> = tokens.collect();
    let head = if w1.is_empty() {
        w0.to_string()
    } else {
        format!("{w0} {w1}")
    };
    (head, rest.join(" "))
}

/// Returns true if `failed` looks like a typo of `success`.
///
/// Runs Damerau-Levenshtein on the first two words only; the rest must match
/// exactly. Falls back to full-string similarity >= `FALLBACK_THRESHOLD` (0.95,
/// fixed) when the head has a typo but the rest differs. The fallback ignores
/// the caller-supplied `threshold` and always applies the stricter 0.95 floor.
///
/// Known limitation: commands differing only in flags/args beyond the first two
/// words (e.g. `git status -v` vs `git status -s`) are never flagged — rest
/// differences are treated as intentional in v1.
pub(crate) fn command_similar(failed: &str, success: &str, threshold: f64) -> bool {
    if failed == success {
        return false;
    }
    let (failed_head, failed_rest) = command_split(failed);
    let (success_head, success_rest) = command_split(success);
    let head_sim = ratio(&failed_head, &success_head);
    if head_sim >= threshold && head_sim < 1.0 {
        if failed_rest == success_rest {
            return true;
        }
        return ratio(failed, success) >= FALLBACK_THRESHOLD;
    }
    false
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

    #[test]
    fn command_similar_flags_subcommand_typo() {
        assert!(command_similar("git statsu", "git status", 0.8));
    }

    #[test]
    fn command_similar_does_not_flag_different_branches() {
        assert!(!command_similar(
            "git push origin feature-1",
            "git push origin feature-2",
            0.8
        ));
    }

    #[test]
    fn threshold_boundary_inclusive() {
        // ratio exactly at threshold must be flagged (>= is inclusive)
        let sim = ratio("git statsu", "git status");
        assert!(command_similar("git statsu", "git status", sim));
    }

    #[test]
    fn identical_head_different_rest_not_flagged() {
        // head identical → head_sim == 1.0, fails `< 1.0` guard → not flagged
        assert!(!command_similar(
            "git push origin main",
            "git push origin dev",
            0.8
        ));
    }

    #[test]
    fn fallback_path_fires_when_overall_similarity_high() {
        // head typo ("cmmit" vs "commit") + rest typo ("--amned..." vs "--amend...")
        // 2 edits over 43 chars → overall ratio ≈ 0.953 >= FALLBACK_THRESHOLD (0.95)
        // → flagged via fallback even though rest differs
        assert!(command_similar(
            "git cmmit --amned-this-is-a-long-flag-value",
            "git commit --amend-this-is-a-long-flag-value",
            0.8
        ));
    }

    #[test]
    fn fallback_path_rejects_when_overall_similarity_low() {
        // head typo + rest typo but strings too short:
        // "git cmmit --amned" (17 chars) vs "git commit --amend" (18 chars)
        // 2 edits / 18 = ratio ≈ 0.889 < FALLBACK_THRESHOLD → not flagged
        assert!(!command_similar(
            "git cmmit --amned",
            "git commit --amend",
            0.8
        ));
    }
}
