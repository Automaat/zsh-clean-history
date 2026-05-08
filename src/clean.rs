use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Local;
use fs2::FileExt;
use tempfile::NamedTempFile;

use crate::allowlist::load_allowlist;
use crate::cleaner::Removal;
use crate::{
    CleaningSettings, HistoryEntry, Paths, compact_exits_file, identify_removals, load_exit_codes,
    parse_history_file,
};

pub struct CleanReport {
    pub removals: Vec<Removal>,
    pub total_lines: usize,
}

pub struct LockedHistory {
    file: fs::File,
}

impl LockedHistory {
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("open {} for lock", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock {}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for LockedHistory {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn run_cleanup(
    paths: &Paths,
    settings: &CleaningSettings,
    dry_run: bool,
) -> Result<CleanReport> {
    let _lock = LockedHistory::acquire(&paths.lock_file())?;

    let exit_codes = load_exit_codes(&paths.exits)?;
    let parsed = parse_history_file(&paths.history, &exit_codes)?;
    let allowlist = load_allowlist(&paths.allowlist)?;
    let removals = identify_removals(&parsed, settings, allowlist.as_ref());
    let total_lines = parsed.entries.len();
    let drop_set = removals_set(&removals);

    if !dry_run && !removals.is_empty() {
        let backup = paths.backup_for(&Local::now().format("%Y%m%d-%H%M%S").to_string());
        if paths.history.exists() {
            fs::copy(&paths.history, &backup)
                .with_context(|| format!("create backup at {}", backup.display()))?;
            prune_old_backups(&paths.history, 5)?;
        }
        write_history_atomically(&paths.history, &parsed.entries, &drop_set)?;
    }

    if !dry_run {
        let keep_ts: HashSet<String> = parsed
            .entries
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| {
                if drop_set.contains(&idx) {
                    None
                } else {
                    e.timestamp.clone()
                }
            })
            .collect();
        compact_exits_file(&paths.exits, &keep_ts)?;
    }

    Ok(CleanReport {
        removals,
        total_lines,
    })
}

pub(crate) fn removals_set(removals: &[Removal]) -> HashSet<usize> {
    removals.iter().map(|r| r.line).collect()
}

pub(crate) fn write_history_atomically(
    path: &Path,
    entries: &[HistoryEntry],
    drop_set: &HashSet<usize>,
) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = NamedTempFile::new_in(parent)?;
    for (idx, entry) in entries.iter().enumerate() {
        if drop_set.contains(&idx) {
            continue;
        }
        tmp.write_all(entry.raw.as_bytes())?;
        tmp.write_all(b"\n")?;
    }
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    fs::File::open(parent)?.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub(crate) fn prune_old_backups(history: &Path, keep: usize) -> Result<()> {
    let parent = history.parent().unwrap_or_else(|| Path::new("."));
    let prefix = format!(
        "{}.backup-",
        history
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(".zsh_history")
    );
    let mut backups: Vec<PathBuf> = fs::read_dir(parent)?
        .filter_map(|r| r.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect();
    backups.sort();
    if backups.len() > keep {
        let drop_count = backups.len() - keep;
        for path in backups.into_iter().take(drop_count) {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;

    use tempfile::TempDir;

    use std::sync::Arc;

    use crate::cleaner::Removal;
    use crate::history::HistoryEntry;

    use super::{prune_old_backups, removals_set, write_history_atomically};

    fn make_entry(raw: &str) -> HistoryEntry {
        HistoryEntry {
            raw: raw.to_string(),
            timestamp: None,
            command: None,
        }
    }

    fn make_removal(line: usize) -> Removal {
        Removal {
            line,
            reason: Arc::from("duplicate"),
            command: "test".to_string(),
        }
    }

    #[test]
    fn removals_set_empty() {
        assert!(removals_set(&[]).is_empty());
    }

    #[test]
    fn removals_set_collects_lines() {
        let removals = vec![make_removal(0), make_removal(3), make_removal(7)];
        let set = removals_set(&removals);
        assert!(set.contains(&0));
        assert!(set.contains(&3));
        assert!(set.contains(&7));
        assert!(!set.contains(&1));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn write_history_atomically_skips_dropped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history");
        let entries = vec![
            make_entry(": 1:0;ls"),
            make_entry(": 2:0;pwd"),
            make_entry(": 3:0;echo hi"),
        ];
        let drop_set: HashSet<usize> = [1].into_iter().collect();
        write_history_atomically(&path, &entries, &drop_set).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(": 1:0;ls"));
        assert!(!content.contains(": 2:0;pwd"));
        assert!(content.contains(": 3:0;echo hi"));
    }

    #[test]
    fn write_history_atomically_all_dropped_produces_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history");
        let entries = vec![make_entry(": 1:0;ls"), make_entry(": 2:0;pwd")];
        let drop_set: HashSet<usize> = [0, 1].into_iter().collect();
        write_history_atomically(&path, &entries, &drop_set).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn prune_old_backups_keeps_newest() {
        let dir = TempDir::new().unwrap();
        let history = dir.path().join(".zsh_history");
        fs::write(&history, b"").unwrap();

        for i in 1..=7u32 {
            let backup = dir
                .path()
                .join(format!(".zsh_history.backup-20240101-{i:06}"));
            fs::write(&backup, b"").unwrap();
        }

        prune_old_backups(&history, 3).unwrap();

        let mut remaining: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with(".zsh_history.backup-"))
            .collect();
        remaining.sort();

        assert_eq!(remaining.len(), 3);
        assert!(remaining[0].contains("-000005"));
        assert!(remaining[1].contains("-000006"));
        assert!(remaining[2].contains("-000007"));
    }

    #[test]
    fn prune_old_backups_noop_when_under_limit() {
        let dir = TempDir::new().unwrap();
        let history = dir.path().join(".zsh_history");
        fs::write(&history, b"").unwrap();

        for i in 1..=3u32 {
            let backup = dir
                .path()
                .join(format!(".zsh_history.backup-20240101-{i:06}"));
            fs::write(&backup, b"").unwrap();
        }

        prune_old_backups(&history, 5).unwrap();

        let count = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".zsh_history.backup-")
            })
            .count();

        assert_eq!(count, 3);
    }
}
