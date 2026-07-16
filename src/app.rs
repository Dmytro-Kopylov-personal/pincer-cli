use crate::api::{Comment, Feed, Story};
use std::collections::{HashMap, VecDeque};

const COMMENTS_CACHE_CAPACITY: usize = 24;

pub enum View {
    List,
    Comments,
}

pub struct App {
    pub feed: Feed,
    pub page: u32,
    pub stories: Vec<Story>,
    pub selected: usize,
    pub view: View,
    pub comments: Vec<Comment>,
    pub comment_selected: usize,
    pub comments_loading: bool,
    pub pending_comment_story_id: Option<String>,
    comments_cache: HashMap<String, crate::api::StoryDetail>,
    comments_cache_order: VecDeque<String>,
    wrapped_comments_width: Option<usize>,
    wrapped_comments: Vec<Vec<String>>,
    pub story_detail_title: String,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            feed: Feed::Hottest,
            page: 1,
            stories: Vec::new(),
            selected: 0,
            view: View::List,
            comments: Vec::new(),
            comment_selected: 0,
            comments_loading: false,
            pending_comment_story_id: None,
            comments_cache: HashMap::new(),
            comments_cache_order: VecDeque::new(),
            wrapped_comments_width: None,
            wrapped_comments: Vec::new(),
            story_detail_title: String::new(),
            status: String::from("Loading..."),
            should_quit: false,
        }
    }

    pub fn selected_story(&self) -> Option<&Story> {
        self.stories.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: i32) {
        match self.view {
            View::List => {
                if self.stories.is_empty() {
                    return;
                }
                let len = self.stories.len() as i32;
                let mut idx = self.selected as i32 + delta;
                if idx < 0 {
                    idx = 0;
                }
                if idx >= len {
                    idx = len - 1;
                }
                self.selected = idx as usize;
            }
            View::Comments => {
                if self.comments.is_empty() {
                    return;
                }
                let len = self.comments.len() as i32;
                let mut idx = self.comment_selected as i32 + delta;
                if idx < 0 {
                    idx = 0;
                }
                if idx >= len {
                    idx = len - 1;
                }
                self.comment_selected = idx as usize;
            }
        }
    }

    pub fn jump_top(&mut self) {
        match self.view {
            View::List => self.selected = 0,
            View::Comments => self.comment_selected = 0,
        }
    }

    pub fn jump_bottom(&mut self) {
        match self.view {
            View::List => {
                if !self.stories.is_empty() {
                    self.selected = self.stories.len() - 1;
                }
            }
            View::Comments => {
                if !self.comments.is_empty() {
                    self.comment_selected = self.comments.len() - 1;
                }
            }
        }
    }

    pub fn next_page(&mut self) {
        self.page = self.page.saturating_add(1);
    }

    pub fn prev_page(&mut self) {
        self.page = self.page.saturating_sub(1).max(1);
    }

    pub fn clear_comments_loading(&mut self) {
        self.comments_loading = false;
        self.pending_comment_story_id = None;
    }

    pub fn begin_comments_loading(&mut self, story_short_id: String, loading_title: String) {
        self.comments_loading = true;
        self.pending_comment_story_id = Some(story_short_id);
        self.story_detail_title = loading_title;
        self.comments.clear();
        self.comment_selected = 0;
        self.clear_wrapped_comments();
        self.view = View::Comments;
        self.status = String::from("Loading comments...");
    }

    pub fn load_comments_detail(&mut self, detail: crate::api::StoryDetail) {
        self.story_detail_title = detail.title;
        self.comments = detail.comments;
        self.comment_selected = 0;
        self.clear_wrapped_comments();
        self.view = View::Comments;
        self.clear_comments_loading();
        self.status = format!("{} comments", self.comments.len());
    }

    pub fn cached_story_detail(&self, short_id: &str) -> Option<crate::api::StoryDetail> {
        self.comments_cache.get(short_id).cloned()
    }

    pub fn cache_story_detail(&mut self, short_id: String, detail: crate::api::StoryDetail) {
        if self.comments_cache.contains_key(&short_id) {
            self.comments_cache_order.retain(|id| id != &short_id);
        }
        self.comments_cache.insert(short_id.clone(), detail);
        self.comments_cache_order.push_back(short_id);

        while self.comments_cache_order.len() > COMMENTS_CACHE_CAPACITY {
            if let Some(evicted_id) = self.comments_cache_order.pop_front() {
                self.comments_cache.remove(&evicted_id);
            }
        }
    }

    pub fn wrapped_comment_lines(&self, index: usize) -> Option<&Vec<String>> {
        self.wrapped_comments.get(index)
    }

    pub fn ensure_wrapped_comments(&mut self, inner_width: usize, max_indent_level: usize) {
        if self.wrapped_comments_width == Some(inner_width)
            && self.wrapped_comments.len() == self.comments.len()
        {
            return;
        }
        self.wrapped_comments_width = Some(inner_width);
        self.wrapped_comments = self
            .comments
            .iter()
            .map(|comment| {
                let depth_indent = "  ".repeat(comment.depth.min(max_indent_level));
                let body_indent = format!("{}  ", depth_indent);
                let wrap_width = inner_width
                    .saturating_sub(body_indent.chars().count())
                    .max(1);
                wrap_comment_text(
                    &comment.comment_plain,
                    comment.is_deleted,
                    &body_indent,
                    wrap_width,
                )
            })
            .collect();
    }

    fn clear_wrapped_comments(&mut self) {
        self.wrapped_comments_width = None;
        self.wrapped_comments.clear();
    }
}

fn wrap_comment_text(
    body: &str,
    is_deleted: bool,
    body_indent: &str,
    wrap_width: usize,
) -> Vec<String> {
    let base_body = if is_deleted {
        "[deleted]".to_string()
    } else {
        body.trim().to_string()
    };
    let clean = base_body.replace('\r', "");
    let wrap_options = textwrap::Options::new(wrap_width)
        .break_words(true)
        .word_splitter(textwrap::WordSplitter::NoHyphenation);

    let mut lines = Vec::new();
    for paragraph in clean.lines() {
        let normalized = paragraph.replace('\t', "    ");
        let trimmed = normalized.trim_end();
        if trimmed.is_empty() {
            lines.push(body_indent.to_string());
            continue;
        }
        for wrapped in textwrap::wrap(trimmed, &wrap_options) {
            lines.push(format!("{}{}", body_indent, wrapped));
        }
    }

    if lines.is_empty() {
        lines.push(body_indent.to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::api::StoryDetail;

    #[test]
    fn page_navigation_stays_one_based() {
        let mut app = App::new();
        app.prev_page();
        assert_eq!(app.page, 1);

        app.next_page();
        app.next_page();
        assert_eq!(app.page, 3);

        app.prev_page();
        assert_eq!(app.page, 2);
    }

    #[test]
    fn comments_cache_hit_and_evicts_oldest() {
        let mut app = App::new();
        for i in 0..26 {
            let id = format!("s{i}");
            app.cache_story_detail(
                id,
                StoryDetail {
                    title: "t".to_string(),
                    url: "u".to_string(),
                    comments: Vec::new(),
                },
            );
        }

        assert!(app.cached_story_detail("s0").is_none());
        assert!(app.cached_story_detail("s1").is_none());
        assert!(app.cached_story_detail("s2").is_some());
        assert!(app.cached_story_detail("s25").is_some());
    }

    #[test]
    fn wrapped_comments_are_recomputed_for_width_changes() {
        let mut app = App::new();
        app.comments = vec![crate::api::Comment {
            short_id: "c1".to_string(),
            comment_plain: "one two three four five six".to_string(),
            score: 1,
            depth: 0,
            commenting_user: "u".to_string(),
            is_deleted: false,
        }];

        app.ensure_wrapped_comments(20, 6);
        let narrow_len = app.wrapped_comment_lines(0).expect("wrapped").len();
        app.ensure_wrapped_comments(50, 6);
        let wide_len = app.wrapped_comment_lines(0).expect("wrapped").len();

        assert!(narrow_len >= wide_len);
    }
}
