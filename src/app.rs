use crate::api::{Comment, Feed, Story, StoryDetail};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;

const COMMENTS_CACHE_CAPACITY: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    List,
    Comments,
}

/// Explicit UI flow for the terminal app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFlowState {
    List,
    Comments,
    SearchingComments,
    HelpList,
    HelpComments,
    HelpSearchingComments,
    Quitting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    List,
    Comments,
    Search,
    Help,
}

pub struct App {
    pub feed: Feed,
    pub page: u32,
    pub stories: Vec<Story>,
    pub selected: usize,
    pub stories_loading: bool,
    flow: AppFlowState,
    pub comments: Vec<Comment>,
    pub comment_selected: usize,
    pub comments_loading: bool,
    pub pending_comment_story_id: Option<String>,
    pending_stories_request_id: u64,
    comments_cache: HashMap<String, StoryDetail>,
    comments_cache_order: VecDeque<String>,
    wrapped_comments_width: Option<usize>,
    wrapped_comments: Vec<Vec<String>>,
    collapsed_comment_ids: HashSet<String>,
    pub search_input: String,
    pub active_search: Option<String>,
    pub profiling_enabled: bool,
    pub frame_count: u64,
    pub last_comments_load_ms: Option<u128>,
    pub story_detail_title: String,
    pub status: String,
    pub high_contrast: bool,
    stories_cache: HashMap<(Feed, u32), Vec<Story>>,
    prefetch_started_feeds: HashSet<Feed>,
}

impl App {
    pub fn new() -> Self {
        Self {
            feed: Feed::Hottest,
            page: 1,
            stories: Vec::new(),
            selected: 0,
            stories_loading: false,
            flow: AppFlowState::List,
            comments: Vec::new(),
            comment_selected: 0,
            comments_loading: false,
            pending_comment_story_id: None,
            pending_stories_request_id: 0,
            comments_cache: HashMap::new(),
            comments_cache_order: VecDeque::new(),
            wrapped_comments_width: None,
            wrapped_comments: Vec::new(),
            collapsed_comment_ids: HashSet::new(),
            search_input: String::new(),
            active_search: None,
            profiling_enabled: false,
            frame_count: 0,
            last_comments_load_ms: None,
            story_detail_title: String::new(),
            status: String::from("Loading..."),
            high_contrast: env_flag_enabled("PINCER_HIGH_CONTRAST"),
            stories_cache: HashMap::new(),
            prefetch_started_feeds: HashSet::new(),
        }
    }

    #[must_use]
    pub fn selected_story(&self) -> Option<&Story> {
        self.stories.get(self.selected)
    }

    #[must_use]
    pub fn selected_comment(&self) -> Option<&Comment> {
        self.comments.get(self.comment_selected)
    }

    #[must_use]
    pub fn current_view(&self) -> View {
        match self.flow {
            AppFlowState::List | AppFlowState::HelpList => View::List,
            AppFlowState::Comments
            | AppFlowState::SearchingComments
            | AppFlowState::HelpComments
            | AppFlowState::HelpSearchingComments => View::Comments,
            AppFlowState::Quitting => View::List,
        }
    }

    #[must_use]
    pub fn is_help_visible(&self) -> bool {
        matches!(
            self.flow,
            AppFlowState::HelpList
                | AppFlowState::HelpComments
                | AppFlowState::HelpSearchingComments
        )
    }

    #[must_use]
    pub fn is_search_mode(&self) -> bool {
        matches!(
            self.flow,
            AppFlowState::SearchingComments | AppFlowState::HelpSearchingComments
        )
    }

    #[must_use]
    pub fn is_quitting(&self) -> bool {
        matches!(self.flow, AppFlowState::Quitting)
    }

    pub fn move_selection(&mut self, delta: i32) {
        match self.current_view() {
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
                let filtered = self.filtered_comment_indices();
                if filtered.is_empty() {
                    return;
                }
                let pos = filtered
                    .iter()
                    .position(|&i| i == self.comment_selected)
                    .unwrap_or(0) as i32;
                let mut new_pos = pos + delta;
                if new_pos < 0 {
                    new_pos = 0;
                }
                if new_pos >= filtered.len() as i32 {
                    new_pos = filtered.len() as i32 - 1;
                }
                self.comment_selected = filtered[new_pos as usize];
            }
        }
    }

    pub fn jump_top(&mut self) {
        match self.current_view() {
            View::List => self.selected = 0,
            View::Comments => {
                if let Some(first) = self.filtered_comment_indices().first().copied() {
                    self.comment_selected = first;
                }
            }
        }
    }

    pub fn jump_bottom(&mut self) {
        match self.current_view() {
            View::List => {
                if !self.stories.is_empty() {
                    self.selected = self.stories.len() - 1;
                }
            }
            View::Comments => {
                if let Some(last) = self.filtered_comment_indices().last().copied() {
                    self.comment_selected = last;
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

    pub fn begin_stories_loading(&mut self) -> u64 {
        self.stories_loading = true;
        self.pending_stories_request_id = self.pending_stories_request_id.saturating_add(1);
        self.status = format!("Loading {} page {}...", self.feed.label(), self.page);
        self.pending_stories_request_id
    }

    pub fn is_current_stories_request(&self, request_id: u64) -> bool {
        self.stories_loading && request_id == self.pending_stories_request_id
    }

    pub fn finish_stories_loading(&mut self) {
        self.stories_loading = false;
    }

    #[must_use]
    pub fn cached_stories(&self, feed: Feed, page: u32) -> Option<Vec<Story>> {
        self.stories_cache.get(&(feed, page)).cloned()
    }

    pub fn cache_stories(&mut self, feed: Feed, page: u32, stories: Vec<Story>) {
        self.stories_cache.insert((feed, page), stories);
    }

    #[must_use]
    pub fn begin_feed_prefetch(&mut self, feed: Feed) -> bool {
        self.prefetch_started_feeds.insert(feed)
    }

    pub fn invalidate_feed_story_cache(&mut self, feed: Feed) {
        self.stories_cache
            .retain(|(cached_feed, _), _| *cached_feed != feed);
        self.prefetch_started_feeds.remove(&feed);
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
        self.flow = AppFlowState::Comments;
        self.status = String::from("Loading comments...");
    }

    pub fn load_comments_detail(&mut self, detail: StoryDetail) {
        self.story_detail_title = detail.title;
        self.comments = detail.comments;
        self.comment_selected = 0;
        self.collapsed_comment_ids.clear();
        self.clear_wrapped_comments();
        self.flow = AppFlowState::Comments;
        self.clear_comments_loading();
        self.status = format!("{} comments", self.comments.len());
    }

    pub fn load_comments_partial(&mut self, detail: StoryDetail) {
        self.story_detail_title = detail.title;
        self.comments = detail.comments;
        self.comment_selected = 0;
        self.collapsed_comment_ids.clear();
        self.clear_wrapped_comments();
        self.flow = AppFlowState::Comments;
    }

    #[must_use]
    pub fn cached_story_detail(&self, short_id: &str) -> Option<StoryDetail> {
        self.comments_cache.get(short_id).cloned()
    }

    pub fn cache_story_detail(&mut self, short_id: String, detail: StoryDetail) {
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

    pub fn invalidate_story_cache(&mut self, short_id: &str) {
        self.comments_cache.remove(short_id);
        self.comments_cache_order.retain(|id| id != short_id);
    }

    #[must_use]
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

    #[must_use]
    pub fn comment_indices_for_display(&self) -> Vec<usize> {
        self.filtered_comment_indices()
    }

    #[must_use]
    pub fn comment_display_position(&self, actual_index: usize) -> Option<usize> {
        self.filtered_comment_indices()
            .iter()
            .position(|&i| i == actual_index)
    }

    pub fn toggle_selected_comment_collapsed(&mut self) {
        let Some(comment) = self.selected_comment() else {
            return;
        };
        let id = comment.short_id.clone();
        if self.collapsed_comment_ids.contains(&id) {
            self.collapsed_comment_ids.remove(&id);
        } else {
            self.collapsed_comment_ids.insert(id);
        }
    }

    pub fn is_comment_collapsed(&self, actual_index: usize) -> bool {
        self.comments
            .get(actual_index)
            .map(|c| self.collapsed_comment_ids.contains(&c.short_id))
            .unwrap_or(false)
    }

    pub fn start_search_mode(&mut self) {
        if matches!(self.flow, AppFlowState::Comments) {
            self.flow = AppFlowState::SearchingComments;
            self.search_input = self.active_search.clone().unwrap_or_default();
        }
    }

    pub fn apply_search(&mut self) {
        let query = self.search_input.trim().to_lowercase();
        self.active_search = if query.is_empty() { None } else { Some(query) };
        self.flow = match self.flow {
            AppFlowState::SearchingComments => AppFlowState::Comments,
            AppFlowState::HelpSearchingComments => AppFlowState::HelpComments,
            flow => flow,
        };
        if let Some(first) = self.filtered_comment_indices().first().copied() {
            self.comment_selected = first;
        }
    }

    pub fn clear_search_mode(&mut self) {
        self.flow = match self.flow {
            AppFlowState::SearchingComments => AppFlowState::Comments,
            AppFlowState::HelpSearchingComments => AppFlowState::HelpComments,
            flow => flow,
        };
        self.search_input.clear();
    }

    pub fn next_matching_comment(&mut self) {
        let filtered = self.filtered_comment_indices();
        if filtered.is_empty() {
            return;
        }
        let current_pos = filtered
            .iter()
            .position(|&i| i == self.comment_selected)
            .unwrap_or(0);
        let next_pos = (current_pos + 1) % filtered.len();
        self.comment_selected = filtered[next_pos];
    }

    pub fn next_high_score_comment(&mut self, min_score: i32) -> bool {
        let filtered = self.filtered_comment_indices();
        if filtered.is_empty() {
            return false;
        }

        let start_pos = filtered
            .iter()
            .position(|&i| i == self.comment_selected)
            .map(|p| p + 1)
            .unwrap_or(0);

        for offset in 0..filtered.len() {
            let pos = (start_pos + offset) % filtered.len();
            let idx = filtered[pos];
            if self
                .comments
                .get(idx)
                .map(|c| c.score >= min_score)
                .unwrap_or(false)
            {
                self.comment_selected = idx;
                return true;
            }
        }
        false
    }

    pub fn toggle_help(&mut self) {
        self.flow = match self.flow {
            AppFlowState::List => AppFlowState::HelpList,
            AppFlowState::Comments => AppFlowState::HelpComments,
            AppFlowState::SearchingComments => AppFlowState::HelpSearchingComments,
            AppFlowState::HelpList => AppFlowState::List,
            AppFlowState::HelpComments => AppFlowState::Comments,
            AppFlowState::HelpSearchingComments => AppFlowState::SearchingComments,
            AppFlowState::Quitting => AppFlowState::Quitting,
        };
    }

    pub fn return_to_list(&mut self) {
        self.flow = AppFlowState::List;
    }

    pub fn enter_comments_view(&mut self) {
        self.flow = AppFlowState::Comments;
    }

    pub fn request_quit(&mut self) {
        self.flow = AppFlowState::Quitting;
    }

    #[must_use]
    pub fn mode(&self) -> UiMode {
        match self.flow {
            AppFlowState::List => UiMode::List,
            AppFlowState::Comments => UiMode::Comments,
            AppFlowState::SearchingComments => UiMode::Search,
            AppFlowState::HelpList
            | AppFlowState::HelpComments
            | AppFlowState::HelpSearchingComments => UiMode::Help,
            AppFlowState::Quitting => UiMode::List,
        }
    }

    #[must_use]
    pub fn mode_label(&self) -> &'static str {
        match self.mode() {
            UiMode::List => "LIST",
            UiMode::Comments => "COMMENTS",
            UiMode::Search => "SEARCH",
            UiMode::Help => "HELP",
        }
    }

    #[must_use]
    pub fn mode_banner_text(&self) -> String {
        let contrast_mode = if self.high_contrast {
            "HIGH"
        } else {
            "DEFAULT"
        };
        format!(" MODE {} | CONTRAST {} ", self.mode_label(), contrast_mode)
    }

    pub fn tick_frame(&mut self) {
        self.frame_count = self.frame_count.saturating_add(1);
    }

    pub fn toggle_profiling(&mut self) {
        self.profiling_enabled = !self.profiling_enabled;
    }

    fn filtered_comment_indices(&self) -> Vec<usize> {
        let Some(query) = self.active_search.as_ref() else {
            return (0..self.comments.len()).collect();
        };
        self.comments
            .iter()
            .enumerate()
            .filter(|(_, c)| comment_matches_query(c, query))
            .map(|(i, _)| i)
            .collect()
    }

    fn clear_wrapped_comments(&mut self) {
        self.wrapped_comments_width = None;
        self.wrapped_comments.clear();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn comment_matches_query(comment: &Comment, query: &str) -> bool {
    comment.comment_plain.to_lowercase().contains(query)
        || comment.commenting_user.to_lowercase().contains(query)
}

fn env_flag_enabled(var_name: &str) -> bool {
    matches!(
        env::var(var_name)
            .ok()
            .map(|v| v.trim().to_ascii_lowercase()),
        Some(v) if matches!(v.as_str(), "1" | "true" | "yes" | "on")
    )
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
    use super::{env_flag_enabled, App, UiMode, View};
    use crate::api::{Comment, Feed, StoryDetail};

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
        app.comments = vec![Comment {
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

    #[test]
    fn search_filters_comments_and_moves_selection() {
        let mut app = App::new();
        app.comments = vec![
            Comment {
                short_id: "a".to_string(),
                comment_plain: "hello world".to_string(),
                score: 1,
                depth: 0,
                commenting_user: "x".to_string(),
                is_deleted: false,
            },
            Comment {
                short_id: "b".to_string(),
                comment_plain: "rust tui".to_string(),
                score: 10,
                depth: 0,
                commenting_user: "y".to_string(),
                is_deleted: false,
            },
        ];
        app.search_input = "rust".to_string();
        app.apply_search();

        assert_eq!(app.comment_selected, 1);
        assert_eq!(app.comment_indices_for_display(), vec![1]);
    }

    #[test]
    fn mode_label_tracks_list_comments_search_help() {
        let mut app = App::new();
        assert_eq!(app.mode(), UiMode::List);
        assert_eq!(app.mode_label(), "LIST");

        app.enter_comments_view();
        assert_eq!(app.mode(), UiMode::Comments);
        assert_eq!(app.mode_label(), "COMMENTS");

        app.start_search_mode();
        assert_eq!(app.mode(), UiMode::Search);
        assert_eq!(app.mode_label(), "SEARCH");

        app.toggle_help();
        assert_eq!(app.mode(), UiMode::Help);
        assert_eq!(app.mode_label(), "HELP");
        assert_eq!(app.mode_banner_text(), " MODE HELP | CONTRAST DEFAULT ");
    }

    #[test]
    fn explicit_flow_transitions_follow_the_documented_rules() {
        let mut app = App::new();
        assert!(!app.is_quitting());
        assert_eq!(app.current_view(), View::List);

        app.enter_comments_view();
        assert_eq!(app.current_view(), View::Comments);

        app.start_search_mode();
        assert!(app.is_search_mode());
        assert_eq!(app.mode(), UiMode::Search);

        app.toggle_help();
        assert!(app.is_help_visible());
        assert_eq!(app.mode(), UiMode::Help);

        app.toggle_help();
        assert!(app.is_search_mode());

        app.toggle_help();
        assert!(app.is_help_visible());

        app.clear_search_mode();
        assert_eq!(app.current_view(), View::Comments);
        assert!(!app.is_search_mode());

        app.return_to_list();
        assert_eq!(app.current_view(), View::List);

        app.request_quit();
        assert!(app.is_quitting());
    }

    #[test]
    fn stories_loading_tracks_latest_request_only() {
        let mut app = App::new();
        let first = app.begin_stories_loading();
        let second = app.begin_stories_loading();

        assert!(app.stories_loading);
        assert!(!app.is_current_stories_request(first));
        assert!(app.is_current_stories_request(second));

        app.finish_stories_loading();
        assert!(!app.stories_loading);
        assert!(!app.is_current_stories_request(second));
    }

    #[test]
    fn invalidating_feed_story_cache_removes_only_that_feed() {
        let mut app = App::new();
        app.cache_stories(Feed::Hottest, 1, Vec::new());
        app.cache_stories(Feed::HnTop, 1, Vec::new());

        app.invalidate_feed_story_cache(Feed::Hottest);

        assert!(app.cached_stories(Feed::Hottest, 1).is_none());
        assert!(app.cached_stories(Feed::HnTop, 1).is_some());
    }

    #[test]
    fn partial_comments_keep_loading_until_finalized() {
        let mut app = App::new();
        app.begin_comments_loading("s1".to_string(), "story".to_string());
        app.load_comments_partial(StoryDetail {
            title: "story".to_string(),
            url: "https://example.com".to_string(),
            comments: vec![Comment {
                short_id: "c1".to_string(),
                comment_plain: "hello".to_string(),
                score: 0,
                depth: 0,
                commenting_user: "u1".to_string(),
                is_deleted: false,
            }],
        });

        assert!(app.comments_loading);
        assert_eq!(app.comments.len(), 1);
        assert_eq!(app.current_view(), View::Comments);
    }

    #[test]
    fn high_contrast_env_flag_parsing_is_safe() {
        assert!(!env_flag_enabled("DOES_NOT_EXIST"));
    }
}
