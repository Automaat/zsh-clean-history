use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::history::ParsedHistory;
use crate::secrets::mark_secrets;
use crate::settings::CleaningSettings;
use crate::similarity::{base_command, command_similar};

#[derive(Debug, Clone, Serialize)]
pub struct Removal {
    pub line: usize,
    pub reason: String,
    pub command: String,
}

pub fn identify_removals(parsed: &ParsedHistory, settings: &CleaningSettings) -> Vec<Removal> {
    let mut removals: HashMap<usize, String> = HashMap::new();
    dedup_keep_newest(parsed, &mut removals);
    mark_secrets(parsed, &mut removals);
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

fn failed_similar_to_successful(
    parsed: &ParsedHistory,
    settings: &CleaningSettings,
    removals: &mut HashMap<usize, String>,
) {
    let by_base = group_by_base_strings(parsed.successful_counts.keys());
    for (failed_cmd, &fail_count) in &parsed.failed_counts {
        let base = base_command(failed_cmd);
        let candidates = match by_base.get(base) {
            Some(v) => v,
            None => continue,
        };
        let mut chosen: Option<String> = None;
        for &success_cmd in candidates {
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
            if success_cmd.starts_with(failed_cmd.as_str()) && success_cmd.len() > failed_cmd.len()
            {
                chosen = Some(format!("Failed prefix of '{success_cmd}'"));
                break;
            }
            if command_similar(failed_cmd, success_cmd, settings.similarity_threshold) {
                chosen = Some(format!("Failed similar to '{success_cmd}'"));
                break;
            }
        }
        if let Some(reason) = chosen {
            if let Some(indices) = parsed.cmd_to_lines.get(failed_cmd) {
                for &idx in indices {
                    removals.entry(idx).or_insert_with(|| reason.clone());
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
        if rare_weight > settings.rare_threshold as f64 {
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
    fn feature_branch_not_flagged_as_typo() {
        let h = parse_with_exits(
            ": 1:0;git push origin feature-1\n: 2:0;git push origin feature-2\n: 3:0;git push origin feature-2\n",
            &[("1", 1), ("2", 0), ("3", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
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
        let removals = identify_removals(&h, &CleaningSettings::default());
        let hit = removals
            .iter()
            .any(|r| r.line == 0 && r.reason.contains("Failed similar"));
        assert!(hit);
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
        let removals = identify_removals(&h, &s);
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
        let exits: Vec<(String, i32)> = (0..=40)
            .map(|i| ((old_ts + i).to_string(), 0))
            .collect();
        let exits_ref: Vec<(&str, i32)> = exits.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let h = parse_with_exits(&text, &exits_ref);
        let s = CleaningSettings {
            remove_rare: true,
            ..Default::default()
        };
        let removals = identify_removals(&h, &s);
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
        let removals = identify_removals(&h, &s);
        let rare_lines: Vec<usize> = removals
            .iter()
            .filter(|r| r.reason.contains("Rare variant"))
            .map(|r| r.line)
            .collect();
        assert_eq!(rare_lines, vec![20]);
    }
}
