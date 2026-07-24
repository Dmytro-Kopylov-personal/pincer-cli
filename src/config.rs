use crate::keymap::KeymapPreset;
use anyhow::Context;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PersistedConfig {
    pub keymap: Option<KeymapPreset>,
    pub startup: Option<StartupConfig>,
    pub performance: Option<PerformanceConfig>,
    pub network: Option<NetworkConfig>,
    pub ui: Option<UiConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StartupConfig {
    pub feed: Option<String>,
    pub page: Option<u32>,
    pub restore_feed_page: Option<bool>,
    pub nav_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PerformanceConfig {
    pub prefetch_max_pages: Option<u32>,
    pub hn_progressive_initial_comments: Option<usize>,
    pub hn_progressive_step_comments: Option<usize>,
    pub hn_comments_fetch_concurrency: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NetworkConfig {
    pub connect_timeout_ms: Option<u64>,
    pub request_timeout_ms: Option<u64>,
    pub retry_attempts: Option<usize>,
    pub retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UiConfig {
    pub high_contrast: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::PersistedConfig;

    #[test]
    fn persisted_config_deserializes_nested_sections() {
        let json = r#"
        {
          "keymap": "plain",
          "startup": { "feed": "hn-top", "page": 3, "restore_feed_page": true },
          "performance": {
            "prefetch_max_pages": 15,
            "hn_progressive_initial_comments": 8,
            "hn_progressive_step_comments": 12,
            "hn_comments_fetch_concurrency": 6
          },
          "network": {
            "connect_timeout_ms": 4000,
            "request_timeout_ms": 10000,
            "retry_attempts": 3,
            "retry_backoff_ms": 150
          },
          "ui": { "high_contrast": true }
        }
        "#;

        let parsed: PersistedConfig = serde_json::from_str(json).expect("valid config json");
        assert_eq!(
            parsed.startup.and_then(|s| s.feed),
            Some("hn-top".to_string())
        );
    }
}
