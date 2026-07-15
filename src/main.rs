mod api;
mod app;
mod ui;

use anyhow::Result;
use app::{App, View};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let result = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    refresh_stories(app);

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                handle_key(app, key.code);
            }
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            if matches!(app.view, View::Comments) {
                app.view = View::List;
                app.status = String::from("Ready");
            } else {
                app.should_quit = true;
            }
        }
        KeyCode::Char('j') | KeyCode::Down => app.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_selection(-1),
        KeyCode::Char('g') => app.jump_top(),
        KeyCode::Char('G') => app.jump_bottom(),
        KeyCode::Char('r') => refresh_stories(app),
        KeyCode::Tab => {
            if matches!(app.view, View::List) {
                app.feed = app.feed.cycle();
                app.page = 1;
                refresh_stories(app);
            }
        }
        KeyCode::Enter => {
            if matches!(app.view, View::List) {
                open_comments(app);
            }
        }
        KeyCode::Char('o') => open_main_link(app),
        KeyCode::Char('b') => open_comments_in_browser(app),
        _ => {}
    }
}

fn refresh_stories(app: &mut App) {
    app.status = String::from("Loading...");
    match api::fetch_stories(app.feed, app.page) {
        Ok(stories) => {
            app.stories = stories;
            app.selected = 0;
            app.status = format!("Loaded {} stories", app.stories.len());
        }
        Err(e) => {
            app.status = format!("Error fetching stories: {}", e);
        }
    }
}

fn open_comments(app: &mut App) {
    let short_id = match app.selected_story() {
        Some(s) => s.short_id.clone(),
        None => return,
    };
    app.status = String::from("Loading comments...");
    match api::fetch_story_detail(&short_id) {
        Ok(detail) => {
            app.comments = detail.comments;
            app.comment_selected = 0;
            app.view = View::Comments;
            app.status = format!("{} comments", app.comments.len());
        }
        Err(e) => {
            app.status = format!("Error fetching comments: {}", e);
        }
    }
}

fn open_main_link(app: &mut App) {
    if let Some(story) = app.selected_story() {
        let url = story.url.clone();
        if url.is_empty() {
            app.status = String::from("No link URL for this story");
            return;
        }
        let _ = open::that(url);
    }
}

fn open_comments_in_browser(app: &mut App) {
    if let Some(story) = app.selected_story() {
        let _ = open::that(story.comments_url.clone());
    }
}
