use crate::keymap::KeymapPreset;
use anyhow::Context;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PersistedConfig {
    pub keymap: Option<KeymapPreset>,
}

pub fn load_config() -> anyhow::Result<Option<PersistedConfig>> {
    let path = config_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let state = serde_json::from_str::<PersistedConfig>(&content)
        .with_context(|| format!("parsing config from {}", path.display()))?;
    Ok(Some(state))
}

fn config_file_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/pincer-cli/config.json"))
}
