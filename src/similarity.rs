use std::sync::OnceLock;

use regex::Regex;
use strsim::damerau_levenshtein;

const FALLBACK_THRESHOLD: f64 = 0.95;

static NORM_PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

fn norm_patterns() -> &'static [(Regex, &'static str)] {
    NORM_PATTERNS.get_or_init(|| {
        vec![
            (Regex::new(r"^https?://").unwrap(), "<URL>"),
            (Regex::new(r"^/").unwrap(), "<PATH>"),
            (Regex::new(r"^v\d+\.\d+\.\d+$").unwrap(), "<VER>"),
            (Regex::new(r"^[0-9a-fA-F]{6,}$").unwrap(), "<SHA>"),
        ]
    })
}

/// Strip symmetric single or double shell quotes from a token.
fn strip_shell_quotes(tok: &str) -> &str {
    let b = tok.as_bytes();
    if b.len() >= 2 {
        let (first, last) = (b[0], b[b.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &tok[1..tok.len() - 1];
        }
    }
    tok
}

/// Normalise volatile tokens in a command string for similarity comparison.
///
/// Replaces hex SHAs (≥6 hex chars), absolute paths (`/…`), URLs (`https?://…`),
/// and exact semver tags (`vN.N.N`) with stable placeholders so that structurally
/// identical commands with different concrete values are not mistaken for typos.
/// Symmetric shell quotes are stripped before classification so that quoted args
/// like `"https://…"` or `'/tmp/…'` are normalised the same as unquoted ones.
pub(crate) fn normalize(s: &str) -> String {
    s.split_whitespace()
        .map(|tok| {
            let inner = strip_shell_quotes(tok);
            for (re, placeholder) in norm_patterns() {
                if re.is_match(inner) {
                    return placeholder.to_string();
                }
            }
            tok.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

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

pub(crate) fn bases_within_dl1(a: &str, b: &str) -> bool {
    damerau_levenshtein(a, b) == 1
}

pub struct DamerauLevenshteinMetric;

impl bk_tree::Metric<String> for DamerauLevenshteinMetric {
    fn distance(&self, a: &String, b: &String) -> u32 {
        damerau_levenshtein(a, b) as u32
    }

    fn threshold_distance(&self, a: &String, b: &String, threshold: u32) -> Option<u32> {
        let dist = self.distance(a, b);
        (dist <= threshold).then_some(dist)
    }
}

/// Conservative BK-tree search radius for a given similarity threshold.
/// Over-includes to avoid false negatives; `ratio()` trims false positives.
pub(crate) fn bk_radius(threshold: f64, query_char_count: usize) -> u32 {
    if threshold >= 1.0 || query_char_count == 0 {
        return 0;
    }
    ((1.0 - threshold) / threshold * query_char_count as f64).ceil() as u32
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
    let failed_norm = normalize(failed);
    let success_norm = normalize(success);
    let (failed_head, failed_rest) = command_split(&failed_norm);
    let (success_head, success_rest) = command_split(&success_norm);
    let head_sim = ratio(&failed_head, &success_head);
    if head_sim >= threshold && head_sim < 1.0 {
        if failed_rest == success_rest {
            return true;
        }
        return ratio(&failed_norm, &success_norm) >= FALLBACK_THRESHOLD;
    }
    false
}

#[cfg(test)]
mod tests {
    use bk_tree::Metric;

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
    fn bases_within_dl1_detects_transposition() {
        assert!(bases_within_dl1("gti", "git"));
    }

    #[test]
    fn bases_within_dl1_detects_substitution() {
        assert!(bases_within_dl1("gut", "git"));
    }

    #[test]
    fn bases_within_dl1_rejects_identical() {
        assert!(!bases_within_dl1("git", "git"));
    }

    #[test]
    fn bases_within_dl1_rejects_distance_two() {
        assert!(!bases_within_dl1("cd", "mv"));
        assert!(!bases_within_dl1("gxx", "git"));
    }

    #[test]
    fn base_command_extracts_first_word() {
        assert_eq!(base_command("git status -s"), "git");
        assert_eq!(base_command("ls"), "ls");
        assert_eq!(base_command(""), "");
        assert_eq!(base_command("   spaces   first"), "spaces");
    }

    #[test]
    fn dl_metric_identical() {
        let m = DamerauLevenshteinMetric;
        assert_eq!(m.distance(&"abc".to_string(), &"abc".to_string()), 0);
    }

    #[test]
    fn dl_metric_swap() {
        let m = DamerauLevenshteinMetric;
        assert_eq!(m.distance(&"ab".to_string(), &"ba".to_string()), 1);
    }

    #[test]
    fn dl_metric_insertion() {
        let m = DamerauLevenshteinMetric;
        assert_eq!(m.distance(&"git".to_string(), &"gitt".to_string()), 1);
    }

    #[test]
    fn bk_radius_exact_match() {
        assert_eq!(bk_radius(1.0, 10), 0);
    }

    #[test]
    fn bk_radius_default_threshold() {
        // threshold=0.8, len=10 → ceil((0.2/0.8)*10) = ceil(2.5) = 3
        assert_eq!(bk_radius(0.8, 10), 3);
    }

    #[test]
    fn bk_radius_half() {
        // threshold=0.5, len=6 → ceil((0.5/0.5)*6) = ceil(6.0) = 6
        assert_eq!(bk_radius(0.5, 6), 6);
    }

    #[test]
    fn bk_radius_empty() {
        assert_eq!(bk_radius(0.8, 0), 0);
    }

    #[test]
    fn bk_radius_non_ascii() {
        // "café" has 4 chars, not 5 bytes
        assert_eq!(bk_radius(0.8, "café".chars().count()), 1);
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

    // --- normalize unit tests ---

    #[test]
    fn normalize_hex_sha_replaced() {
        assert_eq!(normalize("git checkout abc123"), "git checkout <SHA>");
    }

    #[test]
    fn normalize_hex_below_min_len_kept() {
        assert_eq!(normalize("foo abcde"), "foo abcde");
    }

    #[test]
    fn normalize_absolute_path_replaced() {
        assert_eq!(normalize("ls /home/user"), "ls <PATH>");
    }

    #[test]
    fn normalize_url_replaced() {
        assert_eq!(normalize("curl https://example.com/api"), "curl <URL>");
    }

    #[test]
    fn normalize_http_url_replaced() {
        assert_eq!(normalize("curl http://example.com"), "curl <URL>");
    }

    #[test]
    fn normalize_semver_replaced() {
        assert_eq!(normalize("git checkout v1.2.3"), "git checkout <VER>");
    }

    // --- command_similar behaviour with normalised tokens ---

    #[test]
    fn sha_args_not_similar() {
        assert!(!command_similar(
            "git checkout abc123",
            "git checkout def456",
            0.8
        ));
    }

    #[test]
    fn path_args_not_similar() {
        // /home/user1 ends up in head (second word) — normalization collapses both
        // to "ls <PATH>" so head_sim == 1.0 and the pair is not flagged
        assert!(!command_similar("ls /home/user1", "ls /home/user2", 0.8));
    }

    #[test]
    fn url_args_not_similar() {
        assert!(!command_similar(
            "curl https://api.example.com/v1",
            "curl https://api.other.com/v2",
            0.8
        ));
    }

    #[test]
    fn typo_with_sha_arg_still_caught() {
        // head typo ("checkot") + same logical arg (both SHA) → still flagged
        assert!(command_similar(
            "git checkot abc123",
            "git checkout abc123",
            0.8
        ));
    }

    // --- quoted token normalization ---

    #[test]
    fn normalize_double_quoted_url_replaced() {
        assert_eq!(normalize(r#"curl "https://api.example.com""#), "curl <URL>");
    }

    #[test]
    fn normalize_single_quoted_path_replaced() {
        assert_eq!(normalize("ls '/home/user'"), "ls <PATH>");
    }

    #[test]
    fn normalize_double_quoted_sha_replaced() {
        assert_eq!(normalize(r#"git checkout "abc1234""#), "git checkout <SHA>");
    }

    #[test]
    fn normalize_single_quoted_semver_replaced() {
        assert_eq!(normalize("git checkout 'v1.2.3'"), "git checkout <VER>");
    }

    // --- semver anchoring ---

    #[test]
    fn normalize_semver_prerelease_not_replaced() {
        assert_eq!(
            normalize("git checkout v1.2.3-rc1"),
            "git checkout v1.2.3-rc1"
        );
    }

    #[test]
    fn normalize_semver_suffix_not_replaced() {
        assert_eq!(
            normalize("git checkout v1.2.3-hotfix"),
            "git checkout v1.2.3-hotfix"
        );
    }

    #[test]
    fn semver_prerelease_branches_not_collapsed() {
        // v1.2.3-rc1 and v1.2.3-rc2 must remain distinct after normalization
        assert!(!command_similar(
            "git checkout v1.2.3-rc1",
            "git checkout v1.2.3-rc2",
            0.8
        ));
    }
}
