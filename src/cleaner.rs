use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use bk_tree::BKTree;
use regex::RegexSet;
use serde::Serialize;
use strsim::damerau_levenshtein;

use crate::history::ParsedHistory;
use crate::secrets::mark_secrets;
use crate::settings::CleaningSettings;
use crate::similarity::{base_command, bases_within_dl1, command_similar};

struct DLMetric;

impl bk_tree::Metric<String> for DLMetric {
    fn distance(&self, a: &String, b: &String) -> u32 {
        damerau_levenshtein(a, b) as u32
    }

    fn threshold_distance(&self, a: &String, b: &String, threshold: u32) -> Option<u32> {
        let dist = self.distance(a, b);
        (dist <= threshold).then_some(dist)
    }
}

struct SuccessBucketIndex {
    sorted: Vec<String>,
    tree: BKTree<String, DLMetric>,
}

impl SuccessBucketIndex {
    fn build(commands: &[&str]) -> Self {
        let mut sorted: Vec<String> = commands.iter().map(|&s| s.to_owned()).collect();
        sorted.sort_unstable();
        let mut tree = BKTree::new(DLMetric);
        for cmd in &sorted {
            tree.add(cmd.clone());
        }
        Self { sorted, tree }
    }
}

fn build_bucket_indices<'a>(
    by_base: &HashMap<&'a str, Vec<&'a str>>,
) -> HashMap<&'a str, SuccessBucketIndex> {
    by_base
        .iter()
        .map(|(&base, cmds)| (base, SuccessBucketIndex::build(cmds)))
        .collect()
}

fn bk_radius(threshold: f64, query_char_count: usize) -> u32 {
    if threshold >= 1.0 || query_char_count == 0 {
        return 0;
    }
    ((1.0 - threshold) / threshold * query_char_count as f64).ceil() as u32
}

#[derive(Debug, Clone, Serialize)]
pub struct Removal {
    pub line: usize,
    pub reason: String,
    pub command: String,
}

pub fn identify_removals(
    parsed: &ParsedHistory,
    settings: &CleaningSettings,
    allowlist: Option<&RegexSet>,
) -> Vec<Removal> {
    let mut removals: HashMap<usize, String> = HashMap::new();
    dedup_keep_newest(parsed, &mut removals);
    mark_secrets(parsed, &mut removals);
    failed_similar_to_successful(parsed, settings, &mut removals);
    cross_base_typos(parsed, &mut removals);
    if settings.remove_rare {
        rare_variants(parsed, settings, &mut removals);
    }
    let mut out: Vec<Removal> = removals
        .into_iter()
        .map(|(idx, reason)| {
            let command = parsed
                .entries
                .get(idx)
                .and_then(|e| e.command.clone())
                .unwrap_or_default();
            Removal {
                line: idx,
                reason,
                command,
            }
        })
        .collect();
    out.sort_by_key(|r| r.line);
    if let Some(set) = allowlist {
        out.retain(|r| r.reason.starts_with("Secret pattern:") || !set.is_match(&r.command));
    }
    out
}

fn dedup_keep_newest(parsed: &ParsedHistory, removals: &mut HashMap<usize, String>) {
    for (cmd, indices) in &parsed.cmd_to_lines {
        if indices.len() <= 1 {
            continue;
        }
        let keep = parsed.last_seen.get(cmd).copied().unwrap_or(indices[0]);
        for &idx in indices {
            if idx != keep {
                removals
                    .entry(idx)
                    .or_insert_with(|| "Duplicate".to_string());
            }
        }
    }
}

fn failed_similar_to_successful(
    parsed: &ParsedHistory,
    settings: &CleaningSettings,
    removals: &mut HashMap<usize, String>,
) {
    let by_base = group_by_base_strings(parsed.successful_counts.keys());
    let indices = build_bucket_indices(&by_base);

    for (failed_cmd, &fail_count) in &parsed.failed_counts {
        let base = base_command(failed_cmd);
        let index = match indices.get(base) {
            Some(idx) => idx,
            None => continue,
        };

        // Phase 1: prefix probe via sorted vec — O(log n + k)
        let mut chosen: Option<String> = None;
        let start = index
            .sorted
            .partition_point(|s| s.as_str() < failed_cmd.as_str());
        'prefix: for success_cmd in &index.sorted[start..] {
            if !success_cmd.starts_with(failed_cmd.as_str()) {
                break;
            }
            if success_cmd.len() <= failed_cmd.len() {
                continue;
            }
            let success_count = parsed
                .successful_counts
                .get(success_cmd)
                .copied()
                .unwrap_or(0);
            if success_count > fail_count {
                chosen = Some(format!("Failed prefix of '{success_cmd}'"));
                break 'prefix;
            }
        }

        // Phase 2: BK-tree similarity probe — prunes via triangle inequality
        if chosen.is_none() {
            let radius = bk_radius(settings.similarity_threshold, failed_cmd.chars().count());
            for (_, success_cmd) in index.tree.find(failed_cmd, radius) {
                if success_cmd == failed_cmd {
                    continue;
                }
                let success_count = parsed
                    .successful_counts
                    .get(success_cmd)
                    .copied()
                    .unwrap_or(0);
                if success_count <= fail_count {
                    continue;
                }
                if command_similar(failed_cmd, success_cmd, settings.similarity_threshold) {
                    chosen = Some(format!("Failed similar to '{success_cmd}'"));
                    break;
                }
            }
        }

        if let Some(reason) = chosen {
            if let Some(line_indices) = parsed.cmd_to_lines.get(failed_cmd) {
                for &idx in line_indices {
                    removals.entry(idx).or_insert_with(|| reason.clone());
                }
            }
        }
    }
}

fn cross_base_typos(parsed: &ParsedHistory, removals: &mut HashMap<usize, String>) {
    // Count bases from successful runs only — failed typos must not inflate "common" bases
    // and must not suppress rare-but-legitimate bases.
    let mut success_base_counts: HashMap<&str, usize> = HashMap::new();
    for (cmd, &n) in &parsed.successful_counts {
        *success_base_counts.entry(base_command(cmd)).or_default() += n;
    }

    // Candidate rare bases: bases that appear in failed_counts but have very few
    // successful runs.  We source them from failed_counts so that a pure-failure
    // typo base (0 successes) is still considered even though it is absent from
    // success_base_counts.
    let mut failed_bases: HashSet<&str> = HashSet::new();
    for cmd in parsed.failed_counts.keys() {
        failed_bases.insert(base_command(cmd));
    }
    let rare_bases: Vec<&str> = failed_bases
        .into_iter()
        .filter(|b| success_base_counts.get(b).copied().unwrap_or(0) <= 2)
        .collect();

    let mut common_bases: Vec<(&str, usize)> = success_base_counts
        .iter()
        .filter(|(_, &c)| c >= 20)
        .map(|(&b, &c)| (b, c))
        .collect();
    common_bases.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    for rare in rare_bases {
        if let Some((common, _)) = common_bases.iter().find(|(c, _)| bases_within_dl1(rare, c)) {
            let reason = format!("Cross-base typo of '{common}'");
            for (cmd, idxs) in &parsed.cmd_to_lines {
                if base_command(cmd) == rare {
                    // Only remove if this specific command has no successful runs —
                    // a successfully-used command is a legitimate distinct tool, not a typo.
                    if parsed.successful_counts.contains_key(cmd) {
                        continue;
                    }
                    for &idx in idxs {
                        removals.entry(idx).or_insert_with(|| reason.clone());
                    }
                }
            }
        }
    }
}

fn time_decay_weight(timestamp_secs: i64, now_secs: i64) -> f64 {
    const SEVEN_DAYS: i64 = 7 * 24 * 3600;
    const THIRTY_DAYS: i64 = 30 * 24 * 3600;
    let age = now_secs.saturating_sub(timestamp_secs).max(0);
    if age <= SEVEN_DAYS {
        1.0
    } else if age <= THIRTY_DAYS {
        0.5
    } else {
        0.1
    }
}

fn rare_variants(
    parsed: &ParsedHistory,
    settings: &CleaningSettings,
    removals: &mut HashMap<usize, String>,
) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut weighted_counts: HashMap<&str, f64> = HashMap::new();
    for entry in &parsed.entries {
        if let (Some(ts_str), Some(cmd)) = (&entry.timestamp, &entry.command) {
            if let Ok(ts) = ts_str.parse::<i64>() {
                let w = time_decay_weight(ts, now_secs);
                *weighted_counts.entry(cmd.as_str()).or_default() += w;
            }
        }
    }

    let by_base = group_by_base_strs(weighted_counts.keys().copied());
    for (&rare_cmd, &rare_weight) in &weighted_counts {
        if rare_weight > settings.rare_threshold {
            continue;
        }
        let base = base_command(rare_cmd);
        let Some(candidates) = by_base.get(base) else {
            continue;
        };
        for &common_cmd in candidates {
            if common_cmd == rare_cmd {
                continue;
            }
            let common_weight = weighted_counts.get(common_cmd).copied().unwrap_or(0.0);
            if common_weight <= rare_weight * 3.0 {
                continue;
            }
            if command_similar(rare_cmd, common_cmd, settings.similarity_threshold) {
                if let Some(indices) = parsed.cmd_to_lines.get(rare_cmd) {
                    let reason = format!("Rare variant of '{common_cmd}'");
                    for &idx in indices {
                        removals.entry(idx).or_insert_with(|| reason.clone());
                    }
                }
                break;
            }
        }
    }
}

fn group_by_base_strings<'a, I>(iter: I) -> HashMap<&'a str, Vec<&'a str>>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut by_base: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
    for s in iter {
        by_base.entry(base_command(s)).or_default().push(s.as_str());
    }
    by_base
}

fn group_by_base_strs<'a, I>(iter: I) -> HashMap<&'a str, Vec<&'a str>>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut by_base: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
    for s in iter {
        by_base.entry(base_command(s)).or_default().push(s);
    }
    by_base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::parse_history_text;

    fn parse_with_exits(text: &str, exits: &[(&str, i32)]) -> ParsedHistory {
        let map = exits
            .iter()
            .map(|(ts, c)| ((*ts).to_string(), *c))
            .collect();
        parse_history_text(text, &map)
    }

    #[test]
    fn dedup_keeps_newest_occurrence() {
        let h = parse_with_exits(
            ": 1:0;ls\n: 2:0;pwd\n: 3:0;ls\n: 4:0;ls\n",
            &[("1", 0), ("2", 0), ("3", 0), ("4", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let lines: Vec<usize> = removals.iter().map(|r| r.line).collect();
        assert_eq!(lines, vec![0, 2]);
        assert!(removals.iter().all(|r| r.reason == "Duplicate"));
    }

    #[test]
    fn failed_similar_marked_for_removal() {
        let h = parse_with_exits(
            ": 1:0;git statsu\n: 2:0;git status\n: 3:0;git status\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let hit = removals
            .iter()
            .any(|r| r.line == 0 && r.reason.contains("Failed similar"));
        assert!(hit);
    }

    #[test]
    fn failed_prefix_marked() {
        let h = parse_with_exits(
            ": 1:0;mise ins\n: 2:0;mise install\n: 3:0;mise install\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let hit = removals
            .iter()
            .any(|r| r.line == 0 && r.reason.contains("Failed prefix"));
        assert!(hit);
    }

    #[test]
    fn different_base_command_not_removed() {
        let h = parse_with_exits(": 1:0;ls -la\n: 2:0;git status\n", &[("1", 1), ("2", 0)]);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        assert!(removals.is_empty());
    }

    #[test]
    fn feature_branch_not_flagged_as_typo() {
        let h = parse_with_exits(
            ": 1:0;git push origin feature-1\n: 2:0;git push origin feature-2\n: 3:0;git push origin feature-2\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let flagged = removals
            .iter()
            .any(|r| r.line == 0 && r.reason.contains("Failed similar"));
        assert!(!flagged);
    }

    #[test]
    fn git_statsu_still_flagged() {
        let h = parse_with_exits(
            ": 1:0;git statsu\n: 2:0;git status\n: 3:0;git status\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let hit = removals
            .iter()
            .any(|r| r.line == 0 && r.reason.contains("Failed similar"));
        assert!(hit);
    }

    fn build_cross_base_history(
        _common_base: &str,
        common_cmd: &str,
        common_n: usize,
        rare_cmds: &[&str],
    ) -> (String, Vec<(String, i32)>) {
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=common_n {
            text.push_str(&format!(": {i}:0;{common_cmd}\n"));
            exits.push((i.to_string(), 0));
        }
        let base = common_n + 1;
        for (j, cmd) in rare_cmds.iter().enumerate() {
            let ts = base + j;
            text.push_str(&format!(": {ts}:0;{cmd}\n"));
            // Typo commands are failures — exit code 1
            exits.push((ts.to_string(), 1));
        }
        (text, exits)
    }

    #[test]
    fn cross_base_typo_flagged() {
        let (text, exits) = build_cross_base_history("git", "git status", 20, &["gti status"]);
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let hit = removals
            .iter()
            .any(|r| r.command == "gti status" && r.reason.contains("Cross-base typo of 'git'"));
        assert!(hit, "gti status should be flagged; removals: {removals:?}");
    }

    #[test]
    fn cross_base_all_commands_flagged() {
        let (text, exits) =
            build_cross_base_history("git", "git status", 20, &["gti status", "gti log"]);
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let flagged: Vec<&str> = removals
            .iter()
            .filter(|r| r.reason.contains("Cross-base typo of 'git'"))
            .map(|r| r.command.as_str())
            .collect();
        assert!(
            flagged.contains(&"gti status"),
            "gti status missing; removals: {removals:?}"
        );
        assert!(
            flagged.contains(&"gti log"),
            "gti log missing; removals: {removals:?}"
        );
    }

    #[test]
    fn cross_base_no_false_positive_short_cmds() {
        // cd (count=20) vs mv (count=1) — DL distance 2, must NOT flag mv
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=20usize {
            text.push_str(&format!(": {i}:0;cd /home\n"));
            exits.push((i.to_string(), 0));
        }
        text.push_str(": 21:0;mv foo bar\n");
        exits.push(("21".to_string(), 0));
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let flagged = removals
            .iter()
            .any(|r| r.reason.contains("Cross-base typo"));
        assert!(!flagged, "mv should not be flagged; removals: {removals:?}");
    }

    #[test]
    fn cross_base_no_false_positive_dl1_successful_tool() {
        // fd is DL-distance 1 from cd; if fd runs successfully it must NOT be removed
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=20usize {
            text.push_str(&format!(": {i}:0;cd /home\n"));
            exits.push((i.to_string(), 0));
        }
        text.push_str(": 21:0;fd foo\n");
        exits.push(("21".to_string(), 0));
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default());
        let flagged = removals
            .iter()
            .any(|r| r.command == "fd foo" && r.reason.contains("Cross-base typo"));
        assert!(
            !flagged,
            "fd (successful) must not be flagged as cd typo; removals: {removals:?}"
        );
    }

    #[test]
    fn cross_base_does_not_override_existing() {
        // gti status appears twice → first gets Duplicate; cross-base should not override
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=20usize {
            text.push_str(&format!(": {i}:0;git status\n"));
            exits.push((i.to_string(), 0));
        }
        text.push_str(": 21:0;gti status\n");
        text.push_str(": 22:0;gti status\n");
        exits.push(("21".to_string(), 0));
        exits.push(("22".to_string(), 0));
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        // line 20 (index) is the older gti status → must be Duplicate, not Cross-base typo
        let dup = removals
            .iter()
            .find(|r| r.command == "gti status" && r.reason == "Duplicate");
        assert!(
            dup.is_some(),
            "older gti status should remain Duplicate; removals: {removals:?}"
        );
    }

    #[test]
    fn cross_base_rare_threshold_boundary() {
        // rare base has 3 successful runs (success count=3 > 2) → must NOT be flagged
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=20usize {
            text.push_str(&format!(": {i}:0;git status\n"));
            exits.push((i.to_string(), 0));
        }
        // 3 successful uses of gti — it's a legitimate (if confusingly-named) tool
        for ts in 21usize..=23 {
            text.push_str(&format!(": {ts}:0;gti status\n"));
            exits.push((ts.to_string(), 0));
        }
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        // gti success count=3 > 2 → not in rare_bases → not flagged
        let flagged = removals
            .iter()
            .any(|r| r.reason.contains("Cross-base typo"));
        assert!(
            !flagged,
            "base with 3 successful runs should not be flagged; removals: {removals:?}"
        );
    }

    #[test]
    fn cross_base_common_threshold_boundary() {
        // common base has count=19 → threshold is >=20, so must NOT flag rare
        let (text, exits) = build_cross_base_history("git", "git status", 19, &["gti status"]);
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let flagged = removals
            .iter()
            .any(|r| r.reason.contains("Cross-base typo"));
        assert!(
            !flagged,
            "common base with count=19 should not trigger; removals: {removals:?}"
        );
    }

    #[test]
    fn cross_base_deterministic_tie_break() {
        // Two common bases at distance 1 from rare: "xit" (count=25) and "zit" (count=25)
        // Rare: "git" (count=1). Lex tie-break: "xit" < "zit" → always "xit"
        // Actually let's use different counts to make it clear: "xit"=30, "zit"=25
        // So highest count wins → "xit"
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=30usize {
            text.push_str(&format!(": {i}:0;xit foo\n"));
            exits.push((i.to_string(), 0));
        }
        for i in 31..=55usize {
            text.push_str(&format!(": {i}:0;zit foo\n"));
            exits.push((i.to_string(), 0));
        }
        text.push_str(": 56:0;git status\n");
        exits.push(("56".to_string(), 0));
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let cross = removals
            .iter()
            .find(|r| r.command == "git status" && r.reason.contains("Cross-base typo"));
        assert!(
            cross.is_some(),
            "git status should be flagged; removals: {removals:?}"
        );
        assert!(
            cross.unwrap().reason.contains("'xit'"),
            "should pick xit (higher count); removals: {removals:?}"
        );
    }

    #[test]
    fn cross_base_aggregation_by_base() {
        // git status (12) + git commit (8) → git base total=20; gti status (1) should be flagged
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=12usize {
            text.push_str(&format!(": {i}:0;git status\n"));
            exits.push((i.to_string(), 0));
        }
        for i in 13..=20usize {
            text.push_str(&format!(": {i}:0;git commit\n"));
            exits.push((i.to_string(), 0));
        }
        text.push_str(": 21:0;gti status\n");
        exits.push(("21".to_string(), 0));
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let hit = removals
            .iter()
            .any(|r| r.command == "gti status" && r.reason.contains("Cross-base typo of 'git'"));
        assert!(
            hit,
            "gti status should be flagged (git total=20 via aggregation); removals: {removals:?}"
        );
    }

    #[test]
    fn cross_base_includes_failed_counts() {
        // 10 git status (successful) + 10 git log (failed) = git base total=20
        // gti status (1, successful) should be flagged
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        for i in 1..=10usize {
            text.push_str(&format!(": {i}:0;git status\n"));
            exits.push((i.to_string(), 0));
        }
        for i in 11..=20usize {
            text.push_str(&format!(": {i}:0;git log\n"));
            exits.push((i.to_string(), 1));
        }
        text.push_str(": 21:0;gti status\n");
        exits.push(("21".to_string(), 0));
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        let hit = removals
            .iter()
            .any(|r| r.command == "gti status" && r.reason.contains("Cross-base typo of 'git'"));
        assert!(
            hit,
            "gti status should be flagged (git total=20 via failed counts); removals: {removals:?}"
        );
    }

    #[test]
    fn time_decay_weight_buckets() {
        let now = 1_000_000_000i64;
        assert_eq!(time_decay_weight(now, now), 1.0);
        assert_eq!(time_decay_weight(now - 3 * 24 * 3600, now), 1.0);
        assert_eq!(time_decay_weight(now - 7 * 24 * 3600, now), 1.0);
        assert_eq!(time_decay_weight(now - 14 * 24 * 3600, now), 0.5);
        assert_eq!(time_decay_weight(now - 30 * 24 * 3600, now), 0.5);
        assert_eq!(time_decay_weight(now - 31 * 24 * 3600, now), 0.1);
        assert_eq!(time_decay_weight(now - 365 * 24 * 3600, now), 0.1);
    }

    #[test]
    fn recent_rare_variant_protected_when_old_common_exists() {
        // recent typo with low weighted count should still be removed if old common vastly outnumbers it
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // 20 old entries (weight 0.1 each = 2.0) of git status
        // 1 recent entry (weight 1.0) of git statsu
        // weighted: status=2.0, statsu=1.0; 2.0 > 1.0*3? No → statsu NOT removed
        let old_ts = now - 40 * 24 * 3600;
        let recent_ts = now - 1;
        let mut text = String::new();
        for i in 0..20 {
            text.push_str(&format!(": {}:0;git status\n", old_ts + i));
        }
        text.push_str(&format!(": {recent_ts}:0;git statsu\n"));
        let exits: Vec<(String, i32)> = (0..20)
            .map(|i| ((old_ts + i).to_string(), 0))
            .chain(std::iter::once((recent_ts.to_string(), 0)))
            .collect();
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let s = CleaningSettings {
            remove_rare: true,
            ..Default::default()
        };
        let removals = identify_removals(&h, &s, None);
        // statsu weighted=1.0, status weighted=2.0; 2.0 <= 1.0*3=3.0 → not removed
        let rare_removed = removals.iter().any(|r| r.reason.contains("Rare variant"));
        assert!(!rare_removed);
    }

    #[test]
    fn old_rare_variant_removed_when_common_dominates() {
        // 40 old entries (weight 0.1 = 4.0) vs 1 old entry (0.1)
        // 4.0 > 0.1 * 3 = 0.3 → removed
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let old_ts = now - 40 * 24 * 3600;
        let mut text = String::new();
        for i in 0..40 {
            text.push_str(&format!(": {}:0;git status\n", old_ts + i));
        }
        text.push_str(&format!(": {}:0;git statsu\n", old_ts + 40));
        let exits: Vec<(String, i32)> = (0..=40).map(|i| ((old_ts + i).to_string(), 0)).collect();
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let s = CleaningSettings {
            remove_rare: true,
            ..Default::default()
        };
        let removals = identify_removals(&h, &s, None);
        let rare_removed = removals.iter().any(|r| r.reason.contains("Rare variant"));
        assert!(rare_removed);
    }

    #[test]
    fn rare_variant_removed_when_enabled() {
        let mut text = String::new();
        for i in 1..=20 {
            text.push_str(&format!(": {i}:0;git status\n"));
        }
        text.push_str(": 21:0;git statsu\n");
        let exits: Vec<(String, i32)> = (1..=21).map(|i| (i.to_string(), 0)).collect();
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let s = CleaningSettings {
            remove_rare: true,
            ..Default::default()
        };
        let removals = identify_removals(&h, &s, None);
        let rare_lines: Vec<usize> = removals
            .iter()
            .filter(|r| r.reason.contains("Rare variant"))
            .map(|r| r.line)
            .collect();
        assert_eq!(rare_lines, vec![20]);
    }

    #[test]
    fn allowlist_protects_matching_commands() {
        let h = parse_with_exits(
            ": 1:0;kubectl get pods\n: 2:0;kubectl get pods\n",
            &[("1", 0), ("2", 0)],
        );
        let without = identify_removals(&h, &CleaningSettings::default(), None);
        assert_eq!(without.len(), 1);

        let allowlist = RegexSet::new(["^kubectl "]).unwrap();
        let with_list = identify_removals(&h, &CleaningSettings::default(), Some(&allowlist));
        assert!(with_list.is_empty());
    }

    #[test]
    fn allowlist_does_not_suppress_secret_removals() {
        let h = parse_with_exits(
            ": 1:0;export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY\n",
            &[("1", 0)],
        );
        let allowlist = RegexSet::new(["^export "]).unwrap();
        let removals = identify_removals(&h, &CleaningSettings::default(), Some(&allowlist));
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.starts_with("Secret pattern:"));
    }

    // --- bk_radius edge cases ---

    #[test]
    fn bk_radius_threshold_one() {
        assert_eq!(bk_radius(1.0, 10), 0);
    }

    #[test]
    fn bk_radius_threshold_half() {
        // ceil((0.5/0.5) * 10) = ceil(10.0) = 10
        assert_eq!(bk_radius(0.5, 10), 10);
    }

    #[test]
    fn bk_radius_empty_query() {
        assert_eq!(bk_radius(0.8, 0), 0);
    }

    #[test]
    fn bk_radius_single_char() {
        // ceil((0.2/0.8) * 1) = ceil(0.25) = 1
        assert_eq!(bk_radius(0.8, 1), 1);
    }

    #[test]
    fn bk_radius_non_ascii_char_count() {
        // "café" = 4 chars, 5 bytes; radius must use char count
        let query = "café";
        let char_count = query.chars().count();
        assert_eq!(char_count, 4);
        // ceil((0.2/0.8) * 4) = ceil(1.0) = 1
        assert_eq!(bk_radius(0.8, char_count), 1);
    }

    // --- parity / behavioral oracle ---

    #[test]
    fn parity_linear_vs_indexed() {
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        let mut ts = 1u64;

        let push_line = |text: &mut String,
                         exits: &mut Vec<(String, i32)>,
                         ts: &mut u64,
                         cmd: &str,
                         exit: i32| {
            text.push_str(&format!(": {ts}:0;{cmd}\n"));
            exits.push((ts.to_string(), exit));
            *ts += 1;
        };

        for _ in 0..5 {
            push_line(&mut text, &mut exits, &mut ts, "git status", 0);
        }
        for _ in 0..5 {
            push_line(&mut text, &mut exits, &mut ts, "git commit", 0);
        }
        push_line(&mut text, &mut exits, &mut ts, "git statsu", 1);
        push_line(&mut text, &mut exits, &mut ts, "git co", 1);
        for _ in 0..5 {
            push_line(
                &mut text,
                &mut exits,
                &mut ts,
                "git push origin feature-2",
                0,
            );
        }
        push_line(
            &mut text,
            &mut exits,
            &mut ts,
            "git push origin feature-1",
            1,
        );
        push_line(
            &mut text,
            &mut exits,
            &mut ts,
            "git completely-unrelated-xyz",
            1,
        );
        push_line(&mut text, &mut exits, &mut ts, "git logg", 0);
        push_line(&mut text, &mut exits, &mut ts, "git logg", 1);

        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);

        let removed_cmds: Vec<&str> = removals.iter().map(|r| r.command.as_str()).collect();

        assert!(
            removals
                .iter()
                .any(|r| r.command == "git statsu" && r.reason.contains("Failed similar")),
            "git statsu not removed; removals: {removals:?}"
        );
        assert!(
            removals
                .iter()
                .any(|r| r.command == "git co" && r.reason.contains("Failed prefix")),
            "git co not removed as prefix; removals: {removals:?}"
        );

        assert!(
            !removals
                .iter()
                .any(|r| r.command == "git push origin feature-1"
                    && (r.reason.contains("Failed similar") || r.reason.contains("Failed prefix"))),
            "git push origin feature-1 incorrectly removed; removals: {removals:?}"
        );
        assert!(
            !removals
                .iter()
                .any(|r| r.command == "git completely-unrelated-xyz"
                    && (r.reason.contains("Failed similar") || r.reason.contains("Failed prefix"))),
            "git completely-unrelated-xyz incorrectly removed; removed_cmds: {removed_cmds:?}"
        );
        assert!(
            !removals.iter().any(|r| r.command == "git logg"
                && (r.reason.contains("Failed similar") || r.reason.contains("Failed prefix"))),
            "git logg incorrectly removed (equal counts); removals: {removals:?}"
        );
    }

    // --- BK-tree regression tests ---

    #[test]
    fn bk_tree_finds_typo_in_large_bucket() {
        let mut text = String::new();
        let mut exits: Vec<(String, i32)> = Vec::new();
        let mut ts = 1u64;
        let subcommands = [
            "git status",
            "git commit",
            "git log",
            "git diff",
            "git push",
            "git pull",
            "git fetch",
            "git rebase",
            "git merge",
            "git stash",
            "git branch",
            "git checkout",
            "git add",
            "git reset",
            "git clean",
            "git clone",
            "git remote",
            "git tag",
            "git show",
            "git blame",
            "git bisect",
            "git cherry-pick",
            "git describe",
            "git format-patch",
            "git am",
            "git apply",
            "git archive",
            "git bundle",
            "git cat-file",
            "git check-attr",
            "git check-ignore",
            "git check-mailmap",
            "git check-ref-format",
            "git column",
            "git config",
            "git credential",
            "git cvsexportcommit",
            "git daemon",
            "git fast-export",
            "git fast-import",
            "git filter-branch",
            "git gc",
            "git get-tar-commit-id",
            "git grep",
            "git hash-object",
            "git help",
            "git instaweb",
            "git interpret-trailers",
            "git log",
            "git ls-files",
        ];
        for cmd in &subcommands {
            for _ in 0..3 {
                text.push_str(&format!(": {ts}:0;{cmd}\n"));
                exits.push((ts.to_string(), 0));
                ts += 1;
            }
        }
        text.push_str(&format!(": {ts}:0;git statsu\n"));
        exits.push((ts.to_string(), 1));

        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        assert!(
            removals
                .iter()
                .any(|r| r.command == "git statsu" && r.reason.contains("Failed similar")),
            "BK-tree missed typo in large bucket; removals: {removals:?}"
        );
    }

    #[test]
    fn bk_tree_prefix_found_without_similarity() {
        let h = parse_with_exits(
            ": 1:0;mise install\n: 2:0;mise install\n: 3:0;mise ins\n",
            &[("1", 0), ("2", 0), ("3", 1)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        assert!(
            removals
                .iter()
                .any(|r| r.command == "mise ins" && r.reason.contains("Failed prefix")),
            "prefix path missed; removals: {removals:?}"
        );
    }

    #[test]
    fn bk_tree_success_count_guard_blocks_removal() {
        let h = parse_with_exits(
            ": 1:0;git status\n: 2:0;git status\n: 3:0;git statsu\n: 4:0;git statsu\n",
            &[("1", 0), ("2", 0), ("3", 1), ("4", 1)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        assert!(
            !removals.iter().any(|r| r.command == "git statsu"
                && (r.reason.contains("Failed similar") || r.reason.contains("Failed prefix"))),
            "equal counts incorrectly triggered removal; removals: {removals:?}"
        );
    }

    #[test]
    fn bk_tree_empty_command_no_panic() {
        assert_eq!(bk_radius(0.8, 0), 0);
    }

    #[test]
    fn bk_tree_non_ascii_similarity() {
        let h = parse_with_exits(
            ": 1:0;café status\n: 2:0;café status\n: 3:0;café statsu\n",
            &[("1", 0), ("2", 0), ("3", 1)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default(), None);
        assert!(
            removals
                .iter()
                .any(|r| r.command == "café statsu" && r.reason.contains("Failed similar")),
            "non-ASCII similarity missed; removals: {removals:?}"
        );
    }
}
