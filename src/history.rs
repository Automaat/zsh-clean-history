use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub raw: String,
    pub timestamp: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Default)]
pub struct ParsedHistory {
    pub entries: Vec<HistoryEntry>,
    pub successful_counts: HashMap<String, usize>,
    pub failed_counts: HashMap<String, usize>,
    pub cmd_to_lines: HashMap<String, Vec<usize>>,
    pub last_seen: HashMap<String, usize>,
}

pub fn parse_history_file(path: &Path, exit_codes: &HashMap<String, i32>) -> Result<ParsedHistory> {
    let bytes = fs::read(path).with_context(|| format!("read history {}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(parse_history_text(&text, exit_codes))
}

pub fn parse_history_text(text: &str, exit_codes: &HashMap<String, i32>) -> ParsedHistory {
    let mut parsed = ParsedHistory::default();
    let raw_lines: Vec<&str> = text.split_inclusive('\n').collect();

    let mut idx = 0usize;
    while idx < raw_lines.len() {
        let first = raw_lines[idx].trim_end_matches('\n');
        let mut raw_full = first.to_string();
        let mut joined = first.to_string();
        let mut consumed = 1usize;

        while ends_with_unescaped_backslash(&joined) && idx + consumed < raw_lines.len() {
            let next = raw_lines[idx + consumed].trim_end_matches('\n');
            raw_full.push('\n');
            raw_full.push_str(next);
            joined.pop();
            joined.push('\n');
            joined.push_str(next);
            consumed += 1;
        }

        let (timestamp, command) = parse_line(&joined);
        parsed.entries.push(HistoryEntry {
            raw: raw_full,
            timestamp: timestamp.clone(),
            command: command.clone(),
        });
        let entry_idx = parsed.entries.len() - 1;

        if let (Some(ts), Some(cmd)) = (timestamp, command) {
            parsed
                .cmd_to_lines
                .entry(cmd.clone())
                .or_default()
                .push(entry_idx);
            parsed.last_seen.insert(cmd.clone(), entry_idx);

            match exit_codes.get(&ts) {
                Some(0) => *parsed.successful_counts.entry(cmd).or_default() += 1,
                Some(_) => *parsed.failed_counts.entry(cmd).or_default() += 1,
                None => {}
            }
        }

        idx += consumed;
    }

    parsed
}

fn ends_with_unescaped_backslash(line: &str) -> bool {
    let mut count = 0usize;
    for ch in line.chars().rev() {
        if ch == '\\' {
            count += 1;
        } else {
            break;
        }
    }
    count % 2 == 1
}

fn parse_line(line: &str) -> (Option<String>, Option<String>) {
    let line = line.strip_prefix(": ").unwrap_or(line);
    if !line.starts_with(|c: char| c.is_ascii_digit()) {
        return (None, None);
    }
    let (ts_part, rest) = match line.split_once(';') {
        Some(parts) => parts,
        None => return (None, None),
    };
    let (ts, _dur) = match ts_part.split_once(':') {
        Some(parts) => parts,
        None => return (None, None),
    };
    if ts.is_empty() || !ts.chars().all(|c| c.is_ascii_digit()) {
        return (None, None);
    }
    let cmd = rest.trim().to_string();
    if cmd.is_empty() {
        return (None, None);
    }
    (Some(ts.to_string()), Some(cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ParsedHistory {
        parse_history_text(text, &HashMap::new())
    }

    #[test]
    fn parses_basic_line() {
        let h = parse(": 1234567890:0;ls -la\n");
        assert_eq!(h.entries.len(), 1);
        assert_eq!(h.entries[0].command.as_deref(), Some("ls -la"));
        assert_eq!(h.entries[0].timestamp.as_deref(), Some("1234567890"));
    }

    #[test]
    fn invalid_line_kept_raw() {
        let h = parse("not a real entry\n");
        assert_eq!(h.entries.len(), 1);
        assert!(h.entries[0].command.is_none());
    }

    #[test]
    fn multiline_command_joins_with_newline() {
        let text = ": 1234567890:0;echo foo \\\nbar baz\n: 1234567891:0;pwd\n";
        let h = parse(text);
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.entries[0].command.as_deref(), Some("echo foo \nbar baz"));
        assert_eq!(h.entries[0].raw, ": 1234567890:0;echo foo \\\nbar baz");
        assert_eq!(h.entries[1].command.as_deref(), Some("pwd"));
        assert_eq!(h.entries[1].raw, ": 1234567891:0;pwd");
    }

    #[test]
    fn escaped_backslash_does_not_continue() {
        let text = ": 1234567890:0;echo done\\\\\n: 1234567891:0;pwd\n";
        let h = parse(text);
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.entries[0].command.as_deref(), Some("echo done\\\\"));
    }

    #[test]
    fn classifies_success_and_failure_by_exit() {
        let mut exits = HashMap::new();
        exits.insert("1".to_string(), 0);
        exits.insert("2".to_string(), 1);
        let h = parse_history_text(": 1:0;ls\n: 2:0;gti status\n", &exits);
        assert_eq!(h.successful_counts.get("ls"), Some(&1));
        assert_eq!(h.failed_counts.get("gti status"), Some(&1));
    }

    #[test]
    fn last_seen_tracks_most_recent_occurrence() {
        let h = parse(": 1:0;ls\n: 2:0;pwd\n: 3:0;ls\n");
        assert_eq!(h.last_seen.get("ls"), Some(&2));
        assert_eq!(h.cmd_to_lines.get("ls"), Some(&vec![0, 2]));
    }
}
