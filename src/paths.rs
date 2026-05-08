use std::path::PathBuf;

use anyhow::{Context, Result};

pub struct Paths {
    pub history: PathBuf,
    pub exits: PathBuf,
    pub log: PathBuf,
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
        })
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
