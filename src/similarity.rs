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
}
