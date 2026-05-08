use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Result;
use chrono::SecondsFormat;
use serde::Serialize;

use crate::cleaner::Removal;
use crate::settings::CleaningSettings;

#[derive(Serialize)]
struct LogEntry<'a> {
    timestamp: String,
    dry_run: bool,
    settings: SettingsView,
    total_lines: usize,
    removed_count: usize,
    reason_counts: BTreeMap<String, usize>,
    removals: &'a [Removal],
}

#[derive(Serialize)]
struct SettingsView {
    similarity: f64,
    rare_threshold: usize,
    remove_rare: bool,
}

pub fn write_log_entry(
    log_path: &Path,
    settings: &CleaningSettings,
    dry_run: bool,
    total_lines: usize,
    removals: &[Removal],
) -> Result<()> {
    let mut reason_counts: BTreeMap<String, usize> = BTreeMap::new();
    for r in removals {
        *reason_counts.entry(r.reason.clone()).or_default() += 1;
    }

    let entry = LogEntry {
        timestamp: chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        dry_run,
        settings: SettingsView {
            similarity: settings.similarity_threshold,
            rare_threshold: settings.rare_threshold,
            remove_rare: settings.remove_rare,
        },
        total_lines,
        removed_count: removals.len(),
        reason_counts,
        removals,
    };

    let json = serde_json::to_string(&entry)?;
    let first_create = !log_path.exists();
    if let Some(parent) = log_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            anyhow::bail!("log directory does not exist: {}", parent.display());
        }
    }

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    f.write_all(json.as_bytes())?;
    f.write_all(b"\n")?;

    if first_create {
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(log_path, fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleaner::Removal;
    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn writes_one_jsonl_entry() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("cleanup.log");
        let settings = CleaningSettings {
            similarity_threshold: 0.85,
            rare_threshold: 2,
            remove_rare: true,
        };
        let removals = vec![Removal {
            line: 3,
            reason: "Failed similar to 'git status'".into(),
            command: "git statsu".into(),
        }];
        write_log_entry(&log, &settings, true, 42, &removals).unwrap();

        let body = fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = body.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1);
        let v: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["total_lines"], 42);
        assert_eq!(v["removed_count"], 1);
        assert_eq!(v["settings"]["similarity"], 0.85);
        assert_eq!(v["reason_counts"]["Failed similar to 'git status'"], 1);
        assert_eq!(v["removals"][0]["command"], "git statsu");
    }

    #[test]
    fn appends_across_calls() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("cleanup.log");
        let settings = CleaningSettings::default();
        write_log_entry(&log, &settings, true, 10, &[]).unwrap();
        write_log_entry(&log, &settings, true, 11, &[]).unwrap();
        let body = fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = body.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn missing_parent_dir_returns_err() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("missing-dir/cleanup.log");
        let settings = CleaningSettings::default();
        let res = write_log_entry(&log, &settings, true, 1, &[]);
        assert!(res.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn first_write_chmods_to_0600() {
        let dir = tempdir().unwrap();
        let log = dir.path().join("cleanup.log");
        let settings = CleaningSettings::default();
        write_log_entry(&log, &settings, true, 1, &[]).unwrap();
        let mode = fs::metadata(&log).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
