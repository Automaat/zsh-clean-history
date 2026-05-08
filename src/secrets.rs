use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::history::ParsedHistory;

static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();

fn patterns() -> &'static [(&'static str, Regex)] {
    PATTERNS.get_or_init(|| {
        vec![
            (
                "AWS secret key",
                Regex::new(r"AWS_SECRET_ACCESS_KEY=[^\s]+").unwrap(),
            ),
            (
                "AWS access key",
                Regex::new(r"AWS_ACCESS_KEY_ID=AKIA[A-Z0-9]{16}").unwrap(),
            ),
            (
                "Bearer token",
                Regex::new(r"Bearer\s+ey[A-Za-z0-9_.-]+").unwrap(),
            ),
            ("password", Regex::new(r"(?i)password=[^\s]+").unwrap()),
            ("token", Regex::new(r"(?i)token=[^\s]+").unwrap()),
            ("API key", Regex::new(r"(?i)api[_-]?key=[^\s]+").unwrap()),
            (
                "hex blob",
                Regex::new(r"=[A-Fa-f0-9]{40,}(?:[^A-Fa-f0-9]|$)").unwrap(),
            ),
            (
                "base64 blob",
                Regex::new(r"=[A-Za-z0-9+/]{40,}={0,2}(?:[^A-Za-z0-9+/=]|$)").unwrap(),
            ),
        ]
    })
}

/// Marks entries containing secret patterns for removal, overriding any prior reason.
pub fn mark_secrets(
    parsed: &ParsedHistory,
    removals: &mut HashMap<usize, (String, Option<String>)>,
) {
    for (idx, entry) in parsed.entries.iter().enumerate() {
        let Some(cmd) = &entry.command else { continue };
        for (name, re) in patterns() {
            if re.is_match(cmd) {
                removals.insert(idx, (format!("Secret pattern: {name}"), None));
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleaner::identify_removals;
    use crate::history::parse_history_text;
    use crate::settings::CleaningSettings;

    fn parse(text: &str, exits: &[(&str, i32)]) -> ParsedHistory {
        let map = exits.iter().map(|(ts, c)| ((*ts).to_string(), *c)).collect();
        parse_history_text(text, &map)
    }

    #[test]
    fn aws_secret_key_removed() {
        let h = parse(
            ": 1:0;export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("AWS secret key"));
    }

    #[test]
    fn bearer_token_removed() {
        let h = parse(
            ": 1:0;curl -H 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9'\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("Bearer token"));
    }

    #[test]
    fn password_eq_removed() {
        let h = parse(
            ": 1:0;curl -d 'username=admin&password=supersecret'\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("password"));
    }

    #[test]
    fn api_key_removed() {
        let h = parse(
            ": 1:0;curl 'https://api.example.com?api_key=mysecretvalue'\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("API key"));
    }

    #[test]
    fn hex_blob_removed() {
        let h = parse(
            ": 1:0;WEBHOOK_SECRET=aabbccddeeff00112233445566778899aabbccdd ./deploy.sh\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("hex blob"));
    }

    #[test]
    fn clean_command_not_removed() {
        let h = parse(": 1:0;git status\n", &[("1", 0)]);
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert!(removals.is_empty());
    }

    #[test]
    fn secret_overrides_duplicate_reason() {
        let h = parse(
            ": 1:0;export TOKEN=secret123abc\n: 2:0;export TOKEN=secret123abc\n",
            &[("1", 0), ("2", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 2);
        assert!(removals
            .iter()
            .all(|r| r.reason.starts_with("Secret pattern:")));
    }
}
