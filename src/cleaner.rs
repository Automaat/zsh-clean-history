use std::collections::HashMap;

use bk_tree::BKTree;
use serde::Serialize;

use crate::history::ParsedHistory;
use crate::settings::CleaningSettings;
use crate::similarity::{DamerauLevenshteinMetric, base_command, bk_radius, ratio};

#[derive(Debug, Clone, Serialize)]
pub struct Removal {
    pub line: usize,
    pub reason: String,
    pub command: String,
}

pub fn identify_removals(parsed: &ParsedHistory, settings: &CleaningSettings) -> Vec<Removal> {
    let mut removals: HashMap<usize, String> = HashMap::new();
    dedup_keep_newest(parsed, &mut removals);
    failed_similar_to_successful(parsed, settings, &mut removals);
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

struct SuccessBucketIndex {
    sorted: Vec<String>,
    tree: BKTree<String, DamerauLevenshteinMetric>,
}

impl SuccessBucketIndex {
    fn build(commands: &[&str]) -> Self {
        let mut sorted: Vec<String> = commands.iter().map(|&s| s.to_owned()).collect();
        sorted.sort_unstable();
        let mut tree = BKTree::new(DamerauLevenshteinMetric);
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
                let sim = ratio(failed_cmd, success_cmd);
                if (settings.similarity_threshold..1.0).contains(&sim) {
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

fn rare_variants(
    parsed: &ParsedHistory,
    settings: &CleaningSettings,
    removals: &mut HashMap<usize, String>,
) {
    let mut all_counts: HashMap<&str, usize> = HashMap::new();
    for (cmd, &n) in &parsed.successful_counts {
        *all_counts.entry(cmd.as_str()).or_default() += n;
    }
    for (cmd, &n) in &parsed.failed_counts {
        *all_counts.entry(cmd.as_str()).or_default() += n;
    }
    let by_base = group_by_base_strs(all_counts.keys().copied());
    for (rare_cmd, &rare_count) in &all_counts {
        if rare_count > settings.rare_threshold {
            continue;
        }
        let base = base_command(rare_cmd);
        let Some(candidates) = by_base.get(base) else {
            continue;
        };
        for &common_cmd in candidates {
            if common_cmd == *rare_cmd {
                continue;
            }
            let common_count = all_counts.get(common_cmd).copied().unwrap_or(0);
            if common_count <= rare_count.saturating_mul(3) {
                continue;
            }
            let sim = ratio(rare_cmd, common_cmd);
            if sim >= settings.similarity_threshold {
                if let Some(indices) = parsed.cmd_to_lines.get(*rare_cmd) {
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
        let removals = identify_removals(&h, &CleaningSettings::default());
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
        let removals = identify_removals(&h, &CleaningSettings::default());
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
        let removals = identify_removals(&h, &CleaningSettings::default());
        let hit = removals
            .iter()
            .any(|r| r.line == 0 && r.reason.contains("Failed prefix"));
        assert!(hit);
    }

    #[test]
    fn different_base_command_not_removed() {
        let h = parse_with_exits(": 1:0;ls -la\n: 2:0;git status\n", &[("1", 1), ("2", 0)]);
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert!(removals.is_empty());
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
        let removals = identify_removals(&h, &s);
        let rare_lines: Vec<usize> = removals
            .iter()
            .filter(|r| r.reason.contains("Rare variant"))
            .map(|r| r.line)
            .collect();
        assert_eq!(rare_lines, vec![20]);
    }

    #[test]
    fn bk_tree_large_bucket_finds_typo() {
        let mut text = String::new();
        let mut exits_vec: Vec<(String, i32)> = Vec::new();
        for i in 0..100usize {
            let ts = i + 1;
            text.push_str(&format!(": {ts}:0;git arg_{i}\n"));
            exits_vec.push((ts.to_string(), 0));
        }
        let ts = 101usize;
        text.push_str(&format!(": {ts}:0;git arg_5x\n"));
        exits_vec.push((ts.to_string(), 1));
        let exits_ref: Vec<(&str, i32)> = exits_vec.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let removals = identify_removals(&h, &CleaningSettings::default());
        let hit = removals
            .iter()
            .any(|r| r.command == "git arg_5x" && r.reason.contains("Failed similar"));
        assert!(hit, "expected git arg_5x to be flagged as failed similar");
    }

    #[test]
    fn prefix_found_without_bk_tree_similarity() {
        let h = parse_with_exits(
            ": 1:0;mise ins\n: 2:0;mise install\n: 3:0;mise install\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        let hit = removals
            .iter()
            .any(|r| r.command == "mise ins" && r.reason.contains("Failed prefix"));
        assert!(hit);
    }

    #[test]
    fn unrelated_same_base_not_flagged() {
        let h = parse_with_exits(
            ": 1:0;git status\n: 2:0;git remote add origin url\n: 3:0;git remote add origin url\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        let flagged = removals.iter().any(|r| r.command == "git status");
        assert!(
            !flagged,
            "unrelated same-base command should not be removed"
        );
    }

    #[test]
    fn count_guard_blocks_removal() {
        let h = parse_with_exits(
            ": 1:0;git statsu\n: 2:0;git statsu\n: 3:0;git status\n",
            &[("1", 1), ("2", 1), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        let flagged = removals.iter().any(|r| r.command == "git statsu");
        assert!(
            !flagged,
            "count guard must block removal when success_count <= fail_count"
        );
    }

    #[test]
    fn empty_command_no_panic() {
        let h = parse_with_exits(
            ": 1:0;\n: 2:0;git status\n: 3:0;git status\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        let flagged = removals.iter().any(|r| r.command.is_empty());
        assert!(!flagged);
    }

    #[test]
    fn non_ascii_similarity() {
        let h = parse_with_exits(
            ": 1:0;git café\n: 2:0;git cafe\n: 3:0;git cafe\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        let hit = removals
            .iter()
            .any(|r| r.command == "git café" && r.reason.contains("Failed similar"));
        assert!(
            hit,
            "non-ascii command should be found via BK-tree similarity"
        );
    }
}
