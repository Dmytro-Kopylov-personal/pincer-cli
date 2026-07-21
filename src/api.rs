use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

const USER_AGENT: &str = "claw (terminal news client; https://github.com/dmytro)";
const HN_PAGE_SIZE: usize = 25;
const HN_COMMENTS_FETCH_CONCURRENCY: usize = 12;
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

pub fn comment_permalink_url(feed: Feed, story_short_id: &str, comment_short_id: &str) -> String {
    match feed.source() {
        Source::Lobsters => format!("https://lobste.rs/s/{story_short_id}#c_{comment_short_id}"),
        Source::HackerNews => format!("https://news.ycombinator.com/item?id={comment_short_id}"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Feed {
    Hottest,
    Newest,
    HnTop,
    HnNew,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Lobsters,
    HackerNews,
}

impl Feed {
    pub fn source(&self) -> Source {
        match self {
            Feed::Hottest | Feed::Newest => Source::Lobsters,
            Feed::HnTop | Feed::HnNew => Source::HackerNews,
        }
    }

    pub fn endpoint(&self) -> &'static str {
        match self {
            Feed::Hottest => "hottest",
            Feed::Newest => "newest",
            Feed::HnTop => "topstories",
            Feed::HnNew => "newstories",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Feed::Hottest => "Lobsters: Hottest",
            Feed::Newest => "Lobsters: Newest",
            Feed::HnTop => "HN: Top",
            Feed::HnNew => "HN: New",
        }
    }

    pub fn cycle(&self) -> Feed {
        match self {
            Feed::Hottest => Feed::Newest,
            Feed::Newest => Feed::HnTop,
            Feed::HnTop => Feed::HnNew,
            Feed::HnNew => Feed::Hottest,
        }
    }
}

pub fn fetch_stories(feed: Feed, page: u32) -> anyhow::Result<Vec<Story>> {
    match feed.source() {
        Source::Lobsters => fetch_lobsters_stories(feed, page),
        Source::HackerNews => fetch_hn_stories(feed, page),
    }
}

pub fn fetch_story_detail(feed: Feed, short_id: &str) -> anyhow::Result<StoryDetail> {
    match feed.source() {
        Source::Lobsters => fetch_lobsters_story_detail(short_id),
        Source::HackerNews => fetch_hn_story_detail(short_id),
    }
}

pub fn fetch_story_detail_preview(
    feed: Feed,
    short_id: &str,
    max_comments: usize,
) -> anyhow::Result<StoryDetail> {
    match feed.source() {
        Source::Lobsters => fetch_lobsters_story_detail(short_id),
        Source::HackerNews => fetch_hn_story_detail_preview(short_id, max_comments),
    }
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

fn fetch_lobsters_stories(feed: Feed, page: u32) -> anyhow::Result<Vec<Story>> {
    let url = stories_feed_url(feed, page);
    get_json_with_retry::<Vec<Story>>(&url, 2)
}

fn fetch_lobsters_story_detail(short_id: &str) -> anyhow::Result<StoryDetail> {
    let url = format!("https://lobste.rs/s/{short_id}.json");
    get_json_with_retry::<StoryDetail>(&url, 2)
}

fn stories_feed_url(feed: Feed, page: u32) -> String {
    if page <= 1 {
        format!("https://lobste.rs/{}.json", feed.endpoint())
    } else {
        match feed {
            Feed::Hottest => format!("https://lobste.rs/page/{}.json", page),
            Feed::Newest => format!("https://lobste.rs/newest/page/{}.json", page),
            Feed::HnTop | Feed::HnNew => {
                format!(
                    "https://hacker-news.firebaseio.com/v0/{}.json",
                    feed.endpoint()
                )
            }
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct HnItem {
    id: u64,
    #[serde(rename = "type")]
    item_type: Option<String>,
    by: Option<String>,
    text: Option<String>,
    kids: Option<Vec<u64>>,
    url: Option<String>,
    score: Option<i32>,
    title: Option<String>,
    descendants: Option<i32>,
    deleted: Option<bool>,
    dead: Option<bool>,
}

fn fetch_hn_stories(feed: Feed, page: u32) -> anyhow::Result<Vec<Story>> {
    let ids_url = format!(
        "https://hacker-news.firebaseio.com/v0/{}.json",
        feed.endpoint()
    );
    let ids = get_json_with_retry::<Vec<u64>>(&ids_url, 2)?;
    let page_index = page.saturating_sub(1) as usize;
    let start = page_index.saturating_mul(HN_PAGE_SIZE);
    if start >= ids.len() {
        return Ok(Vec::new());
    }
    let end = (start + HN_PAGE_SIZE).min(ids.len());
    let mut stories = Vec::with_capacity(end - start);

    for id in &ids[start..end] {
        if let Some(item) = fetch_hn_item(*id)? {
            if item.item_type.as_deref() != Some("story") {
                continue;
            }
            let short_id = item.id.to_string();
            let comments_url = format!("https://news.ycombinator.com/item?id={short_id}");
            let url = item.url.clone().unwrap_or_else(|| comments_url.clone());
            stories.push(Story {
                short_id,
                title: item.title.unwrap_or_else(|| String::from("[no title]")),
                url,
                score: item.score.unwrap_or(0),
                comment_count: item.descendants.unwrap_or(0),
                tags: vec![String::from("hn")],
                submitter_user: item.by.unwrap_or_else(|| String::from("unknown")),
                comments_url,
            });
        }
    }

    Ok(stories)
}

fn fetch_hn_story_detail(short_id: &str) -> anyhow::Result<StoryDetail> {
    let story_id = short_id.parse::<u64>()?;
    let Some(story_item) = fetch_hn_item(story_id)? else {
        return Err(anyhow::anyhow!("HN story not found: {short_id}"));
    };

    let comments_url = format!("https://news.ycombinator.com/item?id={short_id}");
    let mut comments = Vec::new();
    if let Some(kids) = story_item.kids {
        collect_hn_comments(&kids, 0, &mut comments)?;
    }

    Ok(StoryDetail {
        title: story_item
            .title
            .unwrap_or_else(|| String::from("[no title]")),
        url: story_item.url.unwrap_or(comments_url),
        comments,
    })
}

fn fetch_hn_story_detail_preview(
    short_id: &str,
    max_comments: usize,
) -> anyhow::Result<StoryDetail> {
    let story_id = short_id.parse::<u64>()?;
    let Some(story_item) = fetch_hn_item(story_id)? else {
        return Err(anyhow::anyhow!("HN story not found: {short_id}"));
    };

    let comments_url = format!("https://news.ycombinator.com/item?id={short_id}");
    let mut comments = Vec::new();
    if max_comments > 0 {
        if let Some(kids) = story_item.kids.as_deref() {
            let mut remaining = max_comments;
            collect_hn_comments_limited(kids, 0, &mut comments, &mut remaining)?;
        }
    }

    Ok(StoryDetail {
        title: story_item
            .title
            .unwrap_or_else(|| String::from("[no title]")),
        url: story_item.url.unwrap_or(comments_url),
        comments,
    })
}

fn collect_hn_comments(kids: &[u64], depth: usize, out: &mut Vec<Comment>) -> anyhow::Result<()> {
    for chunk in kids.chunks(HN_COMMENTS_FETCH_CONCURRENCY.max(1)) {
        let mut fetched = Vec::with_capacity(chunk.len());
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for id in chunk {
                handles.push(scope.spawn(move || fetch_hn_item(*id)));
            }
            for handle in handles {
                fetched.push(handle.join().expect("HN comment worker thread panicked"));
            }
        });

        for item_result in fetched {
            let item = match item_result {
                Ok(Some(item)) => item,
                Ok(None) => continue,
                Err(_) => continue,
            };
            if item.item_type.as_deref() != Some("comment") {
                continue;
            }
            let is_deleted = item.deleted.unwrap_or(false) || item.dead.unwrap_or(false);
            let plain = item
                .text
                .as_deref()
                .map(html_to_plain_text)
                .unwrap_or_else(|| String::from("[deleted]"));
            out.push(Comment {
                short_id: item.id.to_string(),
                comment_plain: if plain.is_empty() {
                    String::from("[deleted]")
                } else {
                    plain
                },
                score: 0,
                depth,
                commenting_user: item.by.unwrap_or_else(|| String::from("[deleted]")),
                is_deleted,
            });

            if let Some(child_kids) = item.kids {
                collect_hn_comments(&child_kids, depth + 1, out)?;
            }
        }
    }
    Ok(())
}

fn collect_hn_comments_limited(
    kids: &[u64],
    depth: usize,
    out: &mut Vec<Comment>,
    remaining: &mut usize,
) -> anyhow::Result<()> {
    if *remaining == 0 {
        return Ok(());
    }

    for chunk in kids.chunks(HN_COMMENTS_FETCH_CONCURRENCY.max(1)) {
        if *remaining == 0 {
            break;
        }

        let mut fetched = Vec::with_capacity(chunk.len());
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for id in chunk {
                handles.push(scope.spawn(move || fetch_hn_item(*id)));
            }
            for handle in handles {
                fetched.push(handle.join().expect("HN comment worker thread panicked"));
            }
        });

        for item_result in fetched {
            if *remaining == 0 {
                break;
            }
            let item = match item_result {
                Ok(Some(item)) => item,
                Ok(None) => continue,
                Err(_) => continue,
            };
            if item.item_type.as_deref() != Some("comment") {
                continue;
            }
            let is_deleted = item.deleted.unwrap_or(false) || item.dead.unwrap_or(false);
            let plain = item
                .text
                .as_deref()
                .map(html_to_plain_text)
                .unwrap_or_else(|| String::from("[deleted]"));
            out.push(Comment {
                short_id: item.id.to_string(),
                comment_plain: if plain.is_empty() {
                    String::from("[deleted]")
                } else {
                    plain
                },
                score: 0,
                depth,
                commenting_user: item.by.unwrap_or_else(|| String::from("[deleted]")),
                is_deleted,
            });
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                continue;
            }
            if let Some(child_kids) = item.kids {
                collect_hn_comments_limited(&child_kids, depth + 1, out, remaining)?;
            }
        }
    }

    Ok(())
}

fn fetch_hn_item(id: u64) -> anyhow::Result<Option<HnItem>> {
    let url = format!("https://hacker-news.firebaseio.com/v0/item/{id}.json");
    match get_json_with_retry::<HnItem>(&url, 2) {
        Ok(item) => Ok(Some(item)),
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("404") || msg.contains("not found") {
                Ok(None)
            } else {
                Err(err)
            }
        }
    }
}

fn html_to_plain_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                in_tag = true;
            }
            '>' => {
                in_tag = false;
            }
            '&' if !in_tag => {
                let mut entity = String::new();
                for next in chars.by_ref() {
                    entity.push(next);
                    if next == ';' || entity.len() > 10 {
                        break;
                    }
                }
                out.push_str(match entity.as_str() {
                    "amp;" => "&",
                    "lt;" => "<",
                    "gt;" => ">",
                    "quot;" => "\"",
                    "#x27;" => "'",
                    "#39;" => "'",
                    _ => " ",
                });
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }

    out.replace("<p>", "\n\n")
        .replace("</p>", "\n\n")
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{comment_permalink_url, stories_feed_url, Feed};

    #[test]
    fn comment_permalink_uses_story_and_comment_short_ids() {
        assert_eq!(
            comment_permalink_url(Feed::Hottest, "story123", "comment456"),
            "https://lobste.rs/s/story123#c_comment456"
        );
        assert_eq!(
            comment_permalink_url(Feed::HnTop, "story123", "comment456"),
            "https://news.ycombinator.com/item?id=comment456"
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

    #[test]
    fn feed_cycle_includes_hacker_news_variants() {
        assert_eq!(Feed::Hottest.cycle(), Feed::Newest);
        assert_eq!(Feed::Newest.cycle(), Feed::HnTop);
        assert_eq!(Feed::HnTop.cycle(), Feed::HnNew);
        assert_eq!(Feed::HnNew.cycle(), Feed::Hottest);
    }
}
