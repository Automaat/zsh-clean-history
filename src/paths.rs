use std::path::PathBuf;

use anyhow::{Context, Result};

pub struct Paths {
    pub history: PathBuf,
    pub exits: PathBuf,
    pub log: PathBuf,
    pub allowlist: PathBuf,
}

impl Paths {
    pub fn from_home() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set")?;
        Ok(Self {
            history: home.join(".zsh_history"),
            exits: home.join(".zsh_history_exits"),
            log: home.join(".zsh_history_cleanup.log"),
            allowlist: home.join(".zsh_history_keep"),
        })
    }

    pub fn lock_file(&self) -> PathBuf {
        let mut p = self.history.clone();
        let name = format!(
            "{}.lock",
            p.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(".zsh_history"),
        );
        p.set_file_name(name);
        p
    }

    pub fn backup_for(&self, suffix: &str) -> PathBuf {
        let mut p = self.history.clone();
        let name = format!(
            "{}.backup-{}",
            p.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(".zsh_history"),
            suffix,
        );
        p.set_file_name(name);
        p
    }
}
