use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

pub fn load_exit_codes(path: &Path) -> Result<HashMap<String, i32>> {
    let mut out = HashMap::new();
    if !path.exists() {
        return Ok(out);
    }
    let bytes = fs::read(path).with_context(|| format!("read exits {}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((ts, code)) = line.split_once(':') {
            if let Ok(parsed) = code.parse::<i32>() {
                out.insert(ts.to_string(), parsed);
            }
        }
    }
    Ok(out)
}

pub fn append_exit(path: &Path, timestamp: &str, code: i32) -> Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open exits {}", path.display()))?;
    writeln!(f, "{timestamp}:{code}")?;
    Ok(())
}

pub fn compact_exits_file(path: &Path, keep_timestamps: &HashSet<String>) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let exits = load_exit_codes(path)?;
    let total_before = exits.len();
    let kept: Vec<(String, i32)> = exits
        .into_iter()
        .filter(|(ts, _)| {
            // ts may be decimal ("1700000000.123456"); keep_timestamps holds integer seconds
            let prefix = ts.split_once('.').map(|(s, _)| s).unwrap_or(ts.as_str());
            keep_timestamps.contains(prefix)
        })
        .collect();
    let dropped = total_before.saturating_sub(kept.len());

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = NamedTempFile::new_in(parent)?;
    for (ts, code) in &kept {
        writeln!(tmp, "{ts}:{code}")?;
    }
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_valid_codes() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "1:0").unwrap();
        writeln!(f, "2:127").unwrap();
        writeln!(f, "garbage").unwrap();
        writeln!(f, "3:notanint").unwrap();
        writeln!(f, "1700000000.123456:0").unwrap();
        let map = load_exit_codes(f.path()).unwrap();
        assert_eq!(map.get("1"), Some(&0));
        assert_eq!(map.get("2"), Some(&127));
        assert!(!map.contains_key("3"));
        assert_eq!(map.get("1700000000.123456"), Some(&0));
    }

    #[test]
    fn missing_file_returns_empty() {
        let p = Path::new("/nonexistent/path/exits");
        assert!(load_exit_codes(p).unwrap().is_empty());
    }

    #[test]
    fn compact_drops_unknown_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exits");
        std::fs::write(&path, "1:0\n2:1\n3:0\n").unwrap();

        let mut keep = HashSet::new();
        keep.insert("2".to_string());
        let dropped = compact_exits_file(&path, &keep).unwrap();
        assert_eq!(dropped, 2);

        let after = load_exit_codes(&path).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after.get("2"), Some(&1));
    }
}
