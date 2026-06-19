//! Runtime configuration and OS-standard path resolution.
//!
//! All state lives in the platform data dir (no hardcoded paths):
//! `~/Library/Application Support/sauce` on macOS, `%LOCALAPPDATA%\sauce` on
//! Windows, `~/.local/share/sauce` on Linux.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// User-tunable settings, persisted as `config.json` next to the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Restrict recommendations/searches to a gender: `female`, `male`,
    /// `non-binary`, or `any` (default).
    #[serde(default = "default_gender")]
    pub gender_filter: String,
    /// AniList is keyless, but a future authenticated endpoint can use this.
    #[serde(default)]
    pub anilist_token: Option<String>,
}

fn default_gender() -> String {
    "any".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            gender_filter: default_gender(),
            anilist_token: None,
        }
    }
}

/// The app data directory, created if missing.
pub fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_local_dir().ok_or_else(|| anyhow!("could not resolve data directory"))?;
    let dir = base.join("sauce");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("sauce.db"))
}

fn config_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("config.json"))
}

impl Config {
    pub fn load() -> Result<Config> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Returns true when a character's gender passes the current filter.
    pub fn allows(&self, gender: Option<&str>) -> bool {
        match self.gender_filter.to_lowercase().as_str() {
            "any" | "" => true,
            want => gender.map(|g| g.eq_ignore_ascii_case(want)).unwrap_or(true),
        }
    }
}
