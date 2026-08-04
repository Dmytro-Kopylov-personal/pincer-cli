use crate::api::{Comment, Feed, Story, StoryDetail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TTL_FRESH: Duration = Duration::from_secs(60);
const TTL_STALE_MAX: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// Cached value wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CachedStories {
    pub stories: Vec<Story>,
    cached_at: Instant,
}

impl CachedStories {
    pub fn new(stories: Vec<Story>) -> Self {
        Self {
            stories,
            cached_at: Instant::now(),
        }
    }

    /// Fresh enough to serve without any background fetch.
    pub fn is_fresh(&self) -> bool {
        self.cached_at.elapsed() < TTL_FRESH
    }

    /// Too old: treat as cache miss, show loading.
    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed() >= TTL_STALE_MAX
    }
}

// ---------------------------------------------------------------------------
// Disk cache paths
// ---------------------------------------------------------------------------

fn cache_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".config/pincer-cli/cache"))
}

fn stories_cache_path(feed: Feed, page: u32) -> anyhow::Result<PathBuf> {
    let dir = cache_dir()?;
    Ok(dir.join(format!("stories_{}_p{}.json", feed.as_str(), page)))
}

fn comments_cache_path(short_id: &str) -> anyhow::Result<PathBuf> {
    let dir = cache_dir()?;
    Ok(dir.join(format!("comments_{}.json", short_id)))
}

// ---------------------------------------------------------------------------
// Disk save/load for story pages
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SavedStories {
    stories: Vec<Story>,
}

pub fn save_stories_to_disk_bg(feed: Feed, page: u32, stories: Vec<Story>) {
    std::thread::spawn(move || {
        let path = match stories_cache_path(feed, page) {
            Ok(p) => p,
            Err(_) => return,
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = SavedStories { stories };
        if let Ok(json) = serde_json::to_string(&data) {
            let _ = fs::write(&path, json);
        }
    });
}

pub fn load_stories_from_disk(feed: Feed, page: u32) -> Option<CachedStories> {
    let path = stories_cache_path(feed, page).ok()?;
    let json = fs::read_to_string(path).ok()?;
    let saved: SavedStories = serde_json::from_str(&json).ok()?;
    Some(CachedStories::new(saved.stories))
}

// ---------------------------------------------------------------------------
// Disk save/load for story details (comments)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct SavedStoryDetail {
    title: String,
    url: String,
    comments: Vec<Comment>,
}

pub fn save_comments_to_disk(short_id: &str, detail: &StoryDetail) {
    let path = match comments_cache_path(short_id) {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let data = SavedStoryDetail {
        title: detail.title.clone(),
        url: detail.url.clone(),
        comments: detail.comments.clone(),
    };
    if let Ok(json) = serde_json::to_string(&data) {
        let _ = fs::write(&path, json);
    }
}

pub fn save_comments_to_disk_bg(short_id: String, detail: &StoryDetail) {
    let path = match comments_cache_path(&short_id) {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let data = SavedStoryDetail {
        title: detail.title.clone(),
        url: detail.url.clone(),
        comments: detail.comments.clone(),
    };
    if let Ok(json) = serde_json::to_string(&data) {
        std::thread::spawn(move || {
            let _ = fs::write(&path, json);
        });
    }
}

pub fn load_comments_from_disk(short_id: &str) -> Option<StoryDetail> {
    let path = comments_cache_path(short_id).ok()?;
    let json = fs::read_to_string(path).ok()?;
    let saved: SavedStoryDetail = serde_json::from_str(&json).ok()?;
    Some(StoryDetail {
        title: saved.title,
        url: saved.url,
        comments: saved.comments,
    })
}
