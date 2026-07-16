use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use pincer_cli::api;
use pincer_cli::app::{App, View};
use pincer_cli::state;
use pincer_cli::ui;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;
use std::time::Instant;

struct CommentsLoadResult {
    short_id: String,
    result: Result<api::StoryDetail, String>,
    elapsed_ms: u128,
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let mut restored_selection: Option<usize> = None;
    match state::load_state() {
        Ok(Some(saved)) => {
            app.feed = saved.feed;
            app.page = saved.page;
            restored_selection = Some(saved.selected);
        }
        Ok(None) => {}
        Err(e) => app.status = format!("State load warning: {}", e),
    }
    let (comments_tx, comments_rx) = mpsc::channel::<CommentsLoadResult>();
    let result = run(
        &mut terminal,
        &mut app,
        &comments_tx,
        &comments_rx,
        restored_selection,
    );
    if let Err(e) = state::save_state(&state::PersistedState {
        feed: app.feed,
        page: app.page,
        selected: app.selected,
    }) {
        eprintln!("Failed to save state: {e}");
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    comments_tx: &Sender<CommentsLoadResult>,
    comments_rx: &Receiver<CommentsLoadResult>,
    restored_selection: Option<usize>,
) -> Result<()> {
    refresh_stories(app);
    if let Some(saved) = restored_selection {
        if !app.stories.is_empty() {
            app.selected = saved.min(app.stories.len() - 1);
        }
    }

    loop {
        app.tick_frame();
        apply_comments_load_results(app, comments_rx);
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                handle_key(app, key.code, comments_tx);
            }
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, comments_tx: &Sender<CommentsLoadResult>) {
    if app.search_mode && matches!(app.view, View::Comments) {
        match code {
            KeyCode::Esc => app.clear_search_mode(),
            KeyCode::Enter => app.apply_search(),
            KeyCode::Backspace => {
                app.search_input.pop();
            }
            KeyCode::Char(ch) => app.search_input.push(ch),
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Char('?') => app.toggle_help(),
        KeyCode::Char('p') => {
            app.toggle_profiling();
            app.status = if app.profiling_enabled {
                String::from("Profiling mode enabled")
            } else {
                String::from("Profiling mode disabled")
            };
        }
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            if app.show_help {
                app.show_help = false;
            } else if matches!(app.view, View::Comments) {
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
        KeyCode::Char('r') => {
            if matches!(app.view, View::List) {
                refresh_stories(app);
            } else {
                refresh_current_comments(app, comments_tx);
            }
        }
        KeyCode::Char(']') | KeyCode::PageDown => {
            if matches!(app.view, View::List) {
                app.next_page();
                refresh_stories(app);
            }
        }
        KeyCode::Char('[') | KeyCode::PageUp => {
            if matches!(app.view, View::List) {
                app.prev_page();
                refresh_stories(app);
            }
        }
        KeyCode::Tab => {
            if matches!(app.view, View::List) {
                app.feed = app.feed.cycle();
                app.page = 1;
                refresh_stories(app);
            }
        }
        KeyCode::Enter => {
            if matches!(app.view, View::List) {
                open_comments(app, comments_tx);
            }
        }
        KeyCode::Char('o') => open_main_link(app),
        KeyCode::Char('b') => open_comments_in_browser(app),
        KeyCode::Char('c') => {
            if matches!(app.view, View::Comments) {
                open_comment_permalink(app);
            }
        }
        KeyCode::Char('z') => {
            if matches!(app.view, View::Comments) {
                app.toggle_selected_comment_collapsed();
            }
        }
        KeyCode::Char('/') => {
            if matches!(app.view, View::Comments) {
                app.start_search_mode();
            }
        }
        KeyCode::Char('n') => {
            if matches!(app.view, View::Comments) {
                app.next_matching_comment();
            }
        }
        KeyCode::Char('H')
            if matches!(app.view, View::Comments) && !app.next_high_score_comment(5) =>
        {
            app.status = String::from("No matching high-score comment found");
        }
        _ => {}
    }
}

fn apply_comments_load_results(app: &mut App, comments_rx: &Receiver<CommentsLoadResult>) {
    loop {
        match comments_rx.try_recv() {
            Ok(loaded) => {
                let expected = app.pending_comment_story_id.as_deref();
                if expected != Some(loaded.short_id.as_str()) {
                    continue;
                }
                match loaded.result {
                    Ok(detail) => {
                        let comments_len = detail.comments.len();
                        app.cache_story_detail(loaded.short_id, detail.clone());
                        app.load_comments_detail(detail);
                        app.last_comments_load_ms = Some(loaded.elapsed_ms);
                        app.status = format!(
                            "{} comments loaded in {}ms",
                            comments_len, loaded.elapsed_ms
                        );
                    }
                    Err(error_message) => {
                        app.clear_comments_loading();
                        app.status = format!("Error fetching comments: {}", error_message);
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

fn refresh_current_comments(app: &mut App, comments_tx: &Sender<CommentsLoadResult>) {
    let Some(story) = app.selected_story() else {
        app.status = String::from("No story selected");
        return;
    };
    let short_id = story.short_id.clone();
    app.invalidate_story_cache(&short_id);
    open_comments(app, comments_tx);
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

fn open_comments(app: &mut App, comments_tx: &Sender<CommentsLoadResult>) {
    let (short_id, loading_title) = match app.selected_story() {
        Some(s) => (s.short_id.clone(), s.title.clone()),
        None => {
            app.status = String::from("No story selected");
            return;
        }
    };
    if let Some(detail) = app.cached_story_detail(&short_id) {
        app.load_comments_detail(detail);
        app.status = format!("{} comments (cached)", app.comments.len());
        return;
    }

    app.begin_comments_loading(short_id.clone(), loading_title);
    let sender = comments_tx.clone();
    thread::spawn(move || {
        let started = Instant::now();
        let result = api::fetch_story_detail(&short_id).map_err(|e| e.to_string());
        let _ = sender.send(CommentsLoadResult {
            short_id,
            result,
            elapsed_ms: started.elapsed().as_millis(),
        });
    });
}

fn open_main_link(app: &mut App) {
    let story = match app.selected_story() {
        Some(story) => story,
        None => {
            app.status = String::from("No story selected");
            return;
        }
    };
    let url = story.url.clone();
    if url.is_empty() {
        app.status = String::from("No link URL for this story");
        return;
    }
    match open::that(url) {
        Ok(_) => app.status = String::from("Opened story link"),
        Err(e) => app.status = format!("Error opening story link: {}", e),
    }
}

fn open_comments_in_browser(app: &mut App) {
    let story = match app.selected_story() {
        Some(story) => story,
        None => {
            app.status = String::from("No story selected");
            return;
        }
    };
    match open::that(story.comments_url.clone()) {
        Ok(_) => app.status = String::from("Opened comments page"),
        Err(e) => app.status = format!("Error opening comments page: {}", e),
    }
}

fn open_comment_permalink(app: &mut App) {
    let story_short_id = match app.selected_story() {
        Some(s) => s.short_id.clone(),
        None => {
            app.status = String::from("No story selected");
            return;
        }
    };
    let comment = match app.comments.get(app.comment_selected) {
        Some(comment) => comment,
        None => {
            app.status = String::from("No comment selected");
            return;
        }
    };
    let url = api::comment_permalink_url(&story_short_id, &comment.short_id);
    match open::that(url) {
        Ok(_) => app.status = String::from("Opened comment permalink"),
        Err(e) => app.status = format!("Error opening comment permalink: {}", e),
    }
}
