use std::path::Path;

use anyhow::Result;
use regex::RegexSet;

/// Loads `~/.zsh_history_keep`: one regex per line, lines starting with `#` are comments.
/// Returns `None` if the file does not exist.
pub fn load_allowlist(path: &Path) -> Result<Option<RegexSet>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let patterns: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .collect();
    if patterns.is_empty() {
        return Ok(None);
    }
    Ok(Some(RegexSet::new(patterns)?))
}
