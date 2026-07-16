use serde::Deserialize;

const USER_AGENT: &str = "claw (lobste.rs terminal client; https://github.com/dmytro)";

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    let url = format!("https://lobste.rs/{}.json?page={}", feed.endpoint(), page);
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;
    let stories = client
        .get(url)
        .send()?
        .error_for_status()?
        .json::<Vec<Story>>()?;
    Ok(stories)
}

pub fn fetch_story_detail(short_id: &str) -> anyhow::Result<StoryDetail> {
    let url = format!("https://lobste.rs/s/{}.json", short_id);
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;
    let detail = client
        .get(url)
        .send()?
        .error_for_status()?
        .json::<StoryDetail>()?;
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::comment_permalink_url;

    #[test]
    fn comment_permalink_uses_story_and_comment_short_ids() {
        assert_eq!(
            comment_permalink_url("story123", "comment456"),
            "https://lobste.rs/s/story123#c_comment456"
        );
    }
}
