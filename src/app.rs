use crate::api::{Comment, Feed, Story};

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
    pub comment_scroll: u16,
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
            comment_scroll: 0,
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
}
