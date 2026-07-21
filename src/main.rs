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

const PREFETCH_MAX_PAGES: u32 = 20;
const ALL_FEEDS: [api::Feed; 4] = [
    api::Feed::Hottest,
    api::Feed::Newest,
    api::Feed::HnTop,
    api::Feed::HnNew,
];

struct CommentsLoadResult {
    short_id: String,
    stage: CommentsLoadStage,
    result: Result<api::StoryDetail, String>,
    elapsed_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentsLoadStage {
    Partial,
    Final,
}

struct StoriesLoadResult {
    request_id: u64,
    feed: api::Feed,
    requested_page: u32,
    resolved_page: u32,
    fell_back_to_first_page: bool,
    result: Result<Vec<api::Story>, String>,
}

struct StoriesPrefetchResult {
    feed: api::Feed,
    page: u32,
    stories: Vec<api::Story>,
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
    let (stories_tx, stories_rx) = mpsc::channel::<StoriesLoadResult>();
    let (prefetch_tx, prefetch_rx) = mpsc::channel::<StoriesPrefetchResult>();
    refresh_stories(app, &stories_tx, &prefetch_tx, true);
    let mut pending_restored_selection = restored_selection;

    loop {
        app.tick_frame();
        apply_stories_load_results(app, &stories_rx, &prefetch_tx);
        apply_prefetch_results(app, &prefetch_rx);
        if let Some(saved) = pending_restored_selection {
            if !app.stories_loading && !app.stories.is_empty() {
                app.selected = saved.min(app.stories.len() - 1);
                pending_restored_selection = None;
            }
        }
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
                handle_key(
                    app,
                    key.code,
                    &stories_tx,
                    &prefetch_tx,
                    comments_tx,
                    keymap,
                );
            }
        }
    }

    Ok(())
}

fn handle_key(
    app: &mut App,
    code: KeyCode,
    stories_tx: &Sender<StoriesLoadResult>,
    prefetch_tx: &Sender<StoriesPrefetchResult>,
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

    if app.stories_loading
        && matches!(app.current_view(), View::List)
        && !matches!(
            action,
            KeyAction::Quit
                | KeyAction::ToggleHelp
                | KeyAction::Escape
                | KeyAction::ToggleProfiling
        )
    {
        return;
    }

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
                app.invalidate_feed_story_cache(app.feed);
                refresh_stories(app, stories_tx, prefetch_tx, false);
            } else {
                refresh_current_comments(app, comments_tx);
            }
        }
        KeyAction::NextPage => {
            if matches!(app.current_view(), View::List) {
                app.next_page();
                refresh_stories(app, stories_tx, prefetch_tx, true);
            }
        }
        KeyAction::PrevPage => {
            if matches!(app.current_view(), View::List) {
                app.prev_page();
                refresh_stories(app, stories_tx, prefetch_tx, true);
            }
        }
        KeyAction::CycleFeed => {
            if matches!(app.current_view(), View::List) {
                app.feed = app.feed.cycle();
                app.page = 1;
                refresh_stories(app, stories_tx, prefetch_tx, true);
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
                        match loaded.stage {
                            CommentsLoadStage::Partial => {
                                app.load_comments_partial(detail);
                                app.status = format!(
                                    "{} comments loaded in {}ms (loading more...)",
                                    comments_len, loaded.elapsed_ms
                                );
                            }
                            CommentsLoadStage::Final => {
                                app.cache_story_detail(loaded.short_id, detail.clone());
                                app.load_comments_detail(detail);
                                app.last_comments_load_ms = Some(loaded.elapsed_ms);
                                app.status = format!(
                                    "{} comments loaded in {}ms",
                                    comments_len, loaded.elapsed_ms
                                );
                            }
                        }
                    }
                    Err(error_message) => match loaded.stage {
                        CommentsLoadStage::Partial => {}
                        CommentsLoadStage::Final => {
                            app.clear_comments_loading();
                            app.status = format!("Error fetching comments: {}", error_message);
                        }
                    },
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

fn refresh_stories(
    app: &mut App,
    stories_tx: &Sender<StoriesLoadResult>,
    prefetch_tx: &Sender<StoriesPrefetchResult>,
    use_cache: bool,
) {
    if use_cache {
        if let Some(cached) = app.cached_stories(app.feed, app.page) {
            app.stories = cached;
            app.selected = 0;
            app.status = format!("Loaded {} stories (cached)", app.stories.len());
            ensure_feed_prefetch(app, app.feed, prefetch_tx, 2);
            return;
        }
    }

    let request_id = app.begin_stories_loading();
    let feed = app.feed;
    let requested_page = app.page;
    let sender = stories_tx.clone();
    thread::spawn(move || {
        let (resolved_page, fell_back_to_first_page, result) =
            fetch_stories_with_fallback(feed, requested_page);
        let _ = sender.send(StoriesLoadResult {
            request_id,
            feed,
            requested_page,
            resolved_page,
            fell_back_to_first_page,
            result: result.map_err(|e| e.to_string()),
        });
    });
}

fn should_fallback_to_first_page(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("404") || message.contains("not found")
}

fn fetch_stories_with_fallback(
    feed: api::Feed,
    requested_page: u32,
) -> (u32, bool, anyhow::Result<Vec<api::Story>>) {
    match api::fetch_stories(feed, requested_page) {
        Ok(stories) if requested_page > 1 && stories.is_empty() => {
            let fallback = api::fetch_stories(feed, 1);
            (1, true, fallback)
        }
        Ok(stories) => (requested_page, false, Ok(stories)),
        Err(err) if requested_page > 1 && should_fallback_to_first_page(&err) => {
            let fallback = api::fetch_stories(feed, 1);
            (1, true, fallback)
        }
        Err(err) => (requested_page, false, Err(err)),
    }
}

fn apply_stories_load_results(
    app: &mut App,
    stories_rx: &Receiver<StoriesLoadResult>,
    prefetch_tx: &Sender<StoriesPrefetchResult>,
) {
    loop {
        match stories_rx.try_recv() {
            Ok(loaded) => {
                if !app.is_current_stories_request(loaded.request_id) {
                    continue;
                }
                app.finish_stories_loading();
                match loaded.result {
                    Ok(stories) => {
                        app.cache_stories(loaded.feed, loaded.resolved_page, stories.clone());
                        app.page = loaded.resolved_page;
                        app.stories = stories;
                        app.selected = 0;
                        ensure_feed_prefetch(app, loaded.feed, prefetch_tx, 2);
                        for feed in ALL_FEEDS {
                            if feed != loaded.feed {
                                ensure_feed_prefetch(app, feed, prefetch_tx, 1);
                            }
                        }
                        if loaded.fell_back_to_first_page {
                            app.status = format!(
                                "Page {} unavailable; loaded page 1 ({} stories)",
                                loaded.requested_page,
                                app.stories.len()
                            );
                        } else {
                            app.status = format!("Loaded {} stories", app.stories.len());
                        }
                    }
                    Err(error_message) => {
                        app.status = format!("Error fetching stories: {}", error_message);
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

fn ensure_feed_prefetch(
    app: &mut App,
    feed: api::Feed,
    prefetch_tx: &Sender<StoriesPrefetchResult>,
    start_page: u32,
) {
    if !app.begin_feed_prefetch(feed) {
        return;
    }

    let sender = prefetch_tx.clone();
    thread::spawn(move || {
        for page in start_page..=PREFETCH_MAX_PAGES {
            match api::fetch_stories(feed, page) {
                Ok(stories) => {
                    if stories.is_empty() {
                        break;
                    }
                    let _ = sender.send(StoriesPrefetchResult {
                        feed,
                        page,
                        stories,
                    });
                }
                Err(_) => break,
            }
        }
    });
}

fn apply_prefetch_results(app: &mut App, prefetch_rx: &Receiver<StoriesPrefetchResult>) {
    loop {
        match prefetch_rx.try_recv() {
            Ok(prefetched) => {
                app.cache_stories(prefetched.feed, prefetched.page, prefetched.stories);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
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
    let app_feed = app.feed;
    let sender = comments_tx.clone();
    thread::spawn(move || {
        let started = Instant::now();
        if matches!(app_feed.source(), api::Source::HackerNews) {
            let preview_result =
                api::fetch_story_detail_preview(app_feed, &short_id, 10).map_err(|e| e.to_string());
            let _ = sender.send(CommentsLoadResult {
                short_id: short_id.clone(),
                stage: CommentsLoadStage::Partial,
                result: preview_result,
                elapsed_ms: started.elapsed().as_millis(),
            });
        }

        let final_result = api::fetch_story_detail(app_feed, &short_id).map_err(|e| e.to_string());
        let _ = sender.send(CommentsLoadResult {
            short_id,
            stage: CommentsLoadStage::Final,
            result: final_result,
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
    let url = api::comment_permalink_url(app.feed, &story_short_id, &comment.short_id);
    match open::that(url) {
        Ok(_) => app.status = String::from("Opened comment permalink"),
        Err(e) => app.status = format!("Error opening comment permalink: {}", e),
    }
}
