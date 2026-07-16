use crate::api::Feed;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub feed: Feed,
    pub page: u32,
    pub selected: usize,
}

pub fn load_state() -> anyhow::Result<Option<PersistedState>> {
    let path = state_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("reading persisted state from {}", path.display()))?;
    let state = serde_json::from_str::<PersistedState>(&content)
        .with_context(|| format!("parsing persisted state from {}", path.display()))?;
    Ok(Some(state))
}

pub fn save_state(state: &PersistedState) -> anyhow::Result<()> {
    let path = state_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating state directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(state).context("serializing persisted state")?;
    fs::write(&path, json).with_context(|| format!("writing state file {}", path.display()))?;
    Ok(())
}

fn state_file_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/pincer-cli/state.json"))
}
