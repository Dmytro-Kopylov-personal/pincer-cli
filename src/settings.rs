use crate::api;
use crate::app::{App, NavMode};
use crate::config;
use crate::keymap::{Keymap, KeymapPreset};
use std::str::FromStr;

#[derive(Clone, Copy)]
pub struct RuntimeSettings {
    pub startup_feed: api::Feed,
    pub startup_page: u32,
    pub restore_feed_page: bool,
    pub high_contrast: bool,
    pub prefetch_max_pages: u32,
    pub hn_progressive_initial_comments: usize,
    pub hn_progressive_step_comments: usize,
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub retry_attempts: usize,
    pub retry_backoff_ms: u64,
    pub hn_comments_fetch_concurrency: usize,
}

const DEFAULT_PREFETCH_MAX_PAGES: u32 = 20;
const DEFAULT_HN_PROGRESSIVE_INITIAL_COMMENTS: usize = 10;
const DEFAULT_HN_PROGRESSIVE_STEP_COMMENTS: usize = 20;

pub fn resolve(
    app: &mut App,
    persisted_config: Option<&config::PersistedConfig>,
) -> (Keymap, RuntimeSettings) {
    let mut preset = KeymapPreset::default();
    let mut startup_feed = api::Feed::Hottest;
    let mut startup_page = 1_u32;
    let mut restore_feed_page = false;
    let mut high_contrast = app.high_contrast;
    let mut prefetch_max_pages = DEFAULT_PREFETCH_MAX_PAGES;
    let mut hn_progressive_initial_comments = DEFAULT_HN_PROGRESSIVE_INITIAL_COMMENTS;
    let mut hn_progressive_step_comments = DEFAULT_HN_PROGRESSIVE_STEP_COMMENTS;
    let mut connect_timeout_ms = 5_000_u64;
    let mut request_timeout_ms = 12_000_u64;
    let mut retry_attempts = 2_usize;
    let mut retry_backoff_ms = 200_u64;
    let mut hn_comments_fetch_concurrency = 12_usize;

    if let Some(cfg) = persisted_config {
        if let Some(config_preset) = cfg.keymap {
            preset = config_preset;
        }
        if let Some(startup) = cfg.startup.as_ref() {
            if let Some(feed) = startup.feed.as_deref() {
                match api::Feed::from_str(feed) {
                    Ok(parsed) => startup_feed = parsed,
                    Err(err) => app.status = format!("Config warning: {}", err),
                }
            }
            if let Some(page) = startup.page {
                startup_page = page.max(1);
            }
            if let Some(restore) = startup.restore_feed_page {
                restore_feed_page = restore;
            }
            if let Some(ref mode_str) = startup.nav_mode {
                match mode_str.to_ascii_lowercase().as_str() {
                    "infinite" => app.nav_mode = NavMode::Infinite,
                    _ => app.nav_mode = NavMode::Paged,
                }
            }
        }
        if let Some(ui) = cfg.ui.as_ref() {
            if let Some(cfg_high_contrast) = ui.high_contrast {
                high_contrast = cfg_high_contrast;
            }
        }
        if let Some(perf) = cfg.performance.as_ref() {
            if let Some(value) = perf.prefetch_max_pages {
                prefetch_max_pages = value.max(1);
            }
            if let Some(value) = perf.hn_progressive_initial_comments {
                hn_progressive_initial_comments = value.max(1);
            }
            if let Some(value) = perf.hn_progressive_step_comments {
                hn_progressive_step_comments = value.max(1);
            }
            if let Some(value) = perf.hn_comments_fetch_concurrency {
                hn_comments_fetch_concurrency = value.max(1);
            }
        }
        if let Some(network) = cfg.network.as_ref() {
            if let Some(value) = network.connect_timeout_ms {
                connect_timeout_ms = value.max(1);
            }
            if let Some(value) = network.request_timeout_ms {
                request_timeout_ms = value.max(1);
            }
            if let Some(value) = network.retry_attempts {
                retry_attempts = value.max(1);
            }
            if let Some(value) = network.retry_backoff_ms {
                retry_backoff_ms = value;
            }
        }
    }

    if let Ok(value) = std::env::var("PINCER_KEYMAP") {
        match value.parse::<KeymapPreset>() {
            Ok(parsed) => preset = parsed,
            Err(err) => app.status = format!("Config warning: {}", err),
        }
    }
    if let Ok(value) = std::env::var("PINCER_STARTUP_FEED") {
        match api::Feed::from_str(&value) {
            Ok(feed) => startup_feed = feed,
            Err(err) => app.status = format!("Config warning: {}", err),
        }
    }
    if let Ok(value) = std::env::var("PINCER_STARTUP_PAGE") {
        match value.parse::<u32>() {
            Ok(page) => startup_page = page.max(1),
            Err(err) => app.status = format!("Config warning: invalid startup page: {}", err),
        }
    }
    if let Some(value) = parse_env_bool("PINCER_STARTUP_RESTORE_FEED_PAGE", app) {
        restore_feed_page = value;
    }
    if let Some(value) = parse_env_bool("PINCER_HIGH_CONTRAST", app) {
        high_contrast = value;
    }
    if let Some(value) = parse_env_u32("PINCER_PREFETCH_MAX_PAGES", app) {
        prefetch_max_pages = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HN_PROGRESSIVE_INITIAL_COMMENTS", app) {
        hn_progressive_initial_comments = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HN_PROGRESSIVE_STEP_COMMENTS", app) {
        hn_progressive_step_comments = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HN_COMMENTS_FETCH_CONCURRENCY", app) {
        hn_comments_fetch_concurrency = value.max(1);
    }
    if let Some(value) = parse_env_u64("PINCER_HTTP_CONNECT_TIMEOUT_MS", app) {
        connect_timeout_ms = value.max(1);
    }
    if let Some(value) = parse_env_u64("PINCER_HTTP_REQUEST_TIMEOUT_MS", app) {
        request_timeout_ms = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HTTP_RETRY_ATTEMPTS", app) {
        retry_attempts = value.max(1);
    }
    if let Some(value) = parse_env_u64("PINCER_HTTP_RETRY_BACKOFF_MS", app) {
        retry_backoff_ms = value;
    }

    let keymap = Keymap::new(preset);
    if keymap.preset() != KeymapPreset::Vim {
        app.status = format!("Loaded {} keymap preset", keymap.preset().as_str());
    }
    (
        keymap,
        RuntimeSettings {
            startup_feed,
            startup_page,
            restore_feed_page,
            high_contrast,
            prefetch_max_pages,
            hn_progressive_initial_comments,
            hn_progressive_step_comments,
            connect_timeout_ms,
            request_timeout_ms,
            retry_attempts,
            retry_backoff_ms,
            hn_comments_fetch_concurrency,
        },
    )
}

fn parse_env_bool(var_name: &str, app: &mut App) -> Option<bool> {
    let value = std::env::var(var_name).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => {
            app.status = format!("Config warning: invalid boolean for {}", var_name);
            None
        }
    }
}

fn parse_env_u32(var_name: &str, app: &mut App) -> Option<u32> {
    let value = std::env::var(var_name).ok()?;
    match value.parse::<u32>() {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            app.status = format!("Config warning: invalid {}: {}", var_name, err);
            None
        }
    }
}

fn parse_env_u64(var_name: &str, app: &mut App) -> Option<u64> {
    let value = std::env::var(var_name).ok()?;
    match value.parse::<u64>() {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            app.status = format!("Config warning: invalid {}: {}", var_name, err);
            None
        }
    }
}

fn parse_env_usize(var_name: &str, app: &mut App) -> Option<usize> {
    let value = std::env::var(var_name).ok()?;
    match value.parse::<usize>() {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            app.status = format!("Config warning: invalid {}: {}", var_name, err);
            None
        }
    }
}
