use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::history::ParsedHistory;

static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();

fn patterns() -> &'static [(&'static str, Regex)] {
    PATTERNS.get_or_init(|| {
        vec![
            ("password", Regex::new(r"(?i)password=[^\s]+").unwrap()),
            ("token", Regex::new(r"(?i)token=[^\s]{8,}").unwrap()),
            ("API key", Regex::new(r"(?i)api[_-]?key=[^\s]+").unwrap()),
            (
                "Bearer token",
                Regex::new(r"Bearer\s+ey[A-Za-z0-9_.-]+").unwrap(),
            ),
            (
                "GitHub token",
                Regex::new(r"gh[pso]_[A-Za-z0-9]{36}").unwrap(),
            ),
            (
                "GitHub PAT",
                Regex::new(r"github_pat_[A-Za-z0-9_]{82}").unwrap(),
            ),
            (
                "connection string",
                Regex::new(r"\w+://[^@\s]+:[^@\s]+@").unwrap(),
            ),
            (
                "AWS access key",
                Regex::new(r"AWS_ACCESS_KEY_ID=AKIA[A-Z0-9]{16}").unwrap(),
            ),
            (
                "AWS secret key",
                Regex::new(r"AWS_SECRET_ACCESS_KEY=[^\s]+").unwrap(),
            ),
            (
                "hex blob",
                // 64+ chars avoids false positives from 40-char git SHA1 hashes
                Regex::new(r"=[A-Fa-f0-9]{64,}(?:[^A-Fa-f0-9]|$)").unwrap(),
            ),
            (
                "base64 blob",
                Regex::new(r"=[A-Za-z0-9+/]{40,}={0,2}(?:[^A-Za-z0-9+/=]|$)").unwrap(),
            ),
        ]
    })
}

/// Marks entries containing secret patterns for removal, overriding any prior reason.
pub(crate) fn mark_secrets(parsed: &ParsedHistory, removals: &mut HashMap<usize, String>) {
    for (idx, entry) in parsed.entries.iter().enumerate() {
        let Some(cmd) = &entry.command else { continue };
        for (name, re) in patterns() {
            if re.is_match(cmd) {
                removals.insert(idx, format!("Secret pattern: {name}"));
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
        let map = exits
            .iter()
            .map(|(ts, c)| ((*ts).to_string(), *c))
            .collect();
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
    fn aws_access_key_removed() {
        let h = parse(
            ": 1:0;export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("AWS access key"));
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
            ": 1:0;WEBHOOK_SECRET=aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899 ./deploy.sh\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("hex blob"));
    }

    #[test]
    fn git_sha_not_removed() {
        let h = parse(
            ": 1:0;git reset --hard aabbccddeeff00112233445566778899aabbccdd\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert!(removals.is_empty());
    }

    #[test]
    fn short_token_not_removed() {
        let h = parse(": 1:0;git config credential.token=x\n", &[("1", 0)]);
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert!(removals.is_empty());
    }

    #[test]
    fn connection_string_removed() {
        let h = parse(
            ": 1:0;psql postgres://admin:s3cr3tpass@db.example.com/mydb\n",
            &[("1", 0)],
        );
        let removals = identify_removals(&h, &CleaningSettings::default());
        assert_eq!(removals.len(), 1);
        assert!(removals[0].reason.contains("connection string"));
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
        assert!(
            removals
                .iter()
                .all(|r| r.reason.starts_with("Secret pattern:"))
        );
    }
}
