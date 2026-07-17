use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;

const USER_AGENT: &str = "claw (lobste.rs terminal client; https://github.com/dmytro)";
static HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

#[derive(Debug, Deserialize, Clone)]
pub struct Story {
    pub short_id: String,
    pub title: String,
    pub url: String,
    pub score: i32,
    pub comment_count: i32,
    pub tags: Vec<String>,
    pub submitter_user: String,
    pub comments_url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Comment {
    pub short_id: String,
    pub comment_plain: String,
    pub score: i32,
    pub depth: usize,
    pub commenting_user: String,
    #[serde(default)]
    pub is_deleted: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StoryDetail {
    pub title: String,
    pub url: String,
    pub comments: Vec<Comment>,
}

pub fn comment_permalink_url(story_short_id: &str, comment_short_id: &str) -> String {
    format!(
        "https://lobste.rs/s/{}#c_{}",
        story_short_id, comment_short_id
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Feed {
    Hottest,
    Newest,
}

impl Feed {
    pub fn endpoint(&self) -> &'static str {
        match self {
            Feed::Hottest => "hottest",
            Feed::Newest => "newest",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Feed::Hottest => "Hottest",
            Feed::Newest => "Newest",
        }
    }

    pub fn cycle(&self) -> Feed {
        match self {
            Feed::Hottest => Feed::Newest,
            Feed::Newest => Feed::Hottest,
        }
    }
}

pub fn fetch_stories(feed: Feed, page: u32) -> anyhow::Result<Vec<Story>> {
    let url = stories_feed_url(feed, page);
    let stories = get_json_with_retry::<Vec<Story>>(&url, 2)?;
    Ok(stories)
}

pub fn fetch_story_detail(short_id: &str) -> anyhow::Result<StoryDetail> {
    let url = format!("https://lobste.rs/s/{}.json", short_id);
    let detail = get_json_with_retry::<StoryDetail>(&url, 2)?;
    Ok(detail)
}

fn get_json_with_retry<T: DeserializeOwned>(url: &str, attempts: usize) -> anyhow::Result<T> {
    let client = http_client()?;
    let max_attempts = attempts.max(1);
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=max_attempts {
        match client.get(url).send() {
            Ok(resp) => match resp.error_for_status() {
                Ok(ok) => return Ok(ok.json::<T>()?),
                Err(e) => last_error = Some(e.into()),
            },
            Err(e) => last_error = Some(e.into()),
        }
        if attempt < max_attempts {
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("request failed without error details")))
}

fn http_client() -> anyhow::Result<&'static reqwest::blocking::Client> {
    if let Some(client) = HTTP_CLIENT.get() {
        return Ok(client);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .build()?;
    let _ = HTTP_CLIENT.set(client);
    Ok(HTTP_CLIENT
        .get()
        .expect("HTTP client should be initialized after set"))
}

fn stories_feed_url(feed: Feed, page: u32) -> String {
    if page <= 1 {
        format!("https://lobste.rs/{}.json", feed.endpoint())
    } else {
        match feed {
            Feed::Hottest => format!("https://lobste.rs/page/{}.json", page),
            Feed::Newest => format!("https://lobste.rs/newest/page/{}.json", page),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{comment_permalink_url, stories_feed_url, Feed};

    #[test]
    fn comment_permalink_uses_story_and_comment_short_ids() {
        assert_eq!(
            comment_permalink_url("story123", "comment456"),
            "https://lobste.rs/s/story123#c_comment456"
        );
    }

    #[test]
    fn stories_feed_url_uses_base_json_for_first_page() {
        assert_eq!(
            stories_feed_url(Feed::Newest, 1),
            "https://lobste.rs/newest.json"
        );
    }

    #[test]
    fn stories_feed_url_uses_page_path_for_later_pages() {
        assert_eq!(
            stories_feed_url(Feed::Hottest, 2),
            "https://lobste.rs/page/2.json"
        );
        assert_eq!(
            stories_feed_url(Feed::Newest, 3),
            "https://lobste.rs/newest/page/3.json"
        );
    }
}
