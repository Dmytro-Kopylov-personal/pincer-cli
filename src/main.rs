use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use pincer_cli::api;
use pincer_cli::app::{App, View};
use pincer_cli::config;
use pincer_cli::keymap::{KeyAction, KeyContext, Keymap, KeymapPreset};
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
    let keymap = resolve_keymap(&mut app);
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
        keymap,
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
    keymap: Keymap,
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

        if app.is_quitting() {
            break;
        }

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                handle_key(app, key.code, comments_tx, keymap);
            }
        }
    }

    Ok(())
}

fn handle_key(
    app: &mut App,
    code: KeyCode,
    comments_tx: &Sender<CommentsLoadResult>,
    keymap: Keymap,
) {
    if app.is_help_visible() {
        match code {
            KeyCode::Esc | KeyCode::Char('?') => app.toggle_help(),
            KeyCode::Char('q') => app.request_quit(),
            _ => {}
        }
        return;
    }

    if app.is_search_mode() && matches!(app.current_view(), View::Comments) {
        match keymap.action_for(KeyContext::Search, code) {
            Some(KeyAction::SearchCancel) => app.clear_search_mode(),
            Some(KeyAction::SearchApply) => app.apply_search(),
            Some(KeyAction::SearchBackspace) => {
                app.search_input.pop();
            }
            _ if matches!(code, KeyCode::Char(_)) => {
                if let KeyCode::Char(ch) = code {
                    app.search_input.push(ch);
                }
            }
            _ => {}
        }
        return;
    }

    let context = match app.current_view() {
        View::List => KeyContext::List,
        View::Comments => KeyContext::Comments,
    };

    let Some(action) = keymap.action_for(context, code) else {
        return;
    };

    match action {
        KeyAction::ToggleHelp => app.toggle_help(),
        KeyAction::ToggleProfiling => {
            app.toggle_profiling();
            app.status = if app.profiling_enabled {
                String::from("Profiling mode enabled")
            } else {
                String::from("Profiling mode disabled")
            };
        }
        KeyAction::Quit => app.request_quit(),
        KeyAction::Escape => {
            if app.is_help_visible() {
                app.toggle_help();
            } else if matches!(app.current_view(), View::Comments) {
                app.return_to_list();
                app.status = String::from("Ready");
            } else {
                app.request_quit();
            }
        }
        KeyAction::MoveDown => app.move_selection(1),
        KeyAction::MoveUp => app.move_selection(-1),
        KeyAction::JumpTop => app.jump_top(),
        KeyAction::JumpBottom => app.jump_bottom(),
        KeyAction::Refresh => {
            if matches!(app.current_view(), View::List) {
                refresh_stories(app);
            } else {
                refresh_current_comments(app, comments_tx);
            }
        }
        KeyAction::NextPage => {
            if matches!(app.current_view(), View::List) {
                app.next_page();
                refresh_stories(app);
            }
        }
        KeyAction::PrevPage => {
            if matches!(app.current_view(), View::List) {
                app.prev_page();
                refresh_stories(app);
            }
        }
        KeyAction::CycleFeed => {
            if matches!(app.current_view(), View::List) {
                app.feed = app.feed.cycle();
                app.page = 1;
                refresh_stories(app);
            }
        }
        KeyAction::OpenComments => {
            if matches!(app.current_view(), View::List) {
                open_comments(app, comments_tx);
            }
        }
        KeyAction::OpenStoryLink => open_main_link(app),
        KeyAction::OpenCommentsThread => open_comments_in_browser(app),
        KeyAction::OpenCommentPermalink => {
            if matches!(app.current_view(), View::Comments) {
                open_comment_permalink(app);
            }
        }
        KeyAction::ToggleCommentCollapse => {
            if matches!(app.current_view(), View::Comments) {
                app.toggle_selected_comment_collapsed();
            }
        }
        KeyAction::StartSearch => {
            if matches!(app.current_view(), View::Comments) {
                app.start_search_mode();
            }
        }
        KeyAction::NextMatch => {
            if matches!(app.current_view(), View::Comments) {
                app.next_matching_comment();
            }
        }
        KeyAction::NextHighScore => {
            if matches!(app.current_view(), View::Comments) && !app.next_high_score_comment(5) {
                app.status = String::from("No matching high-score comment found");
            }
        }
        KeyAction::SearchCancel | KeyAction::SearchApply | KeyAction::SearchBackspace => {}
    }
}

fn resolve_keymap(app: &mut App) -> Keymap {
    let mut preset = KeymapPreset::default();

    match config::load_config() {
        Ok(Some(cfg)) => {
            if let Some(config_preset) = cfg.keymap {
                preset = config_preset;
            }
        }
        Ok(None) => {}
        Err(e) => app.status = format!("Config load warning: {}", e),
    }

    if let Ok(value) = std::env::var("PINCER_KEYMAP") {
        match value.parse::<KeymapPreset>() {
            Ok(parsed) => preset = parsed,
            Err(err) => app.status = format!("Config warning: {}", err),
        }
    }

    let keymap = Keymap::new(preset);
    if keymap.preset() != KeymapPreset::Vim {
        app.status = format!("Loaded {} keymap preset", keymap.preset().as_str());
    }
    keymap
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
    let requested_page = app.page;
    match api::fetch_stories(app.feed, app.page) {
        Ok(stories) => {
            app.stories = stories;
            app.selected = 0;
            app.status = format!("Loaded {} stories", app.stories.len());
        }
        Err(e) => {
            if requested_page > 1 && should_fallback_to_first_page(&e) {
                match api::fetch_stories(app.feed, 1) {
                    Ok(stories) => {
                        app.page = 1;
                        app.stories = stories;
                        app.selected = 0;
                        app.status = format!(
                            "Page {} unavailable; loaded page 1 ({} stories)",
                            requested_page,
                            app.stories.len()
                        );
                    }
                    Err(fallback_error) => {
                        app.status = format!(
                            "Error fetching stories: {} (fallback failed: {})",
                            e, fallback_error
                        );
                    }
                }
            } else {
                app.status = format!("Error fetching stories: {}", e);
            }
        }
    }
}

fn should_fallback_to_first_page(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("404") || message.contains("not found")
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
