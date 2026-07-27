use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use pincer_cli::api;
use pincer_cli::app::{App, NavMode, View};
use pincer_cli::cache;
use pincer_cli::config;
use pincer_cli::keymap::{KeyAction, KeyContext, Keymap, KeymapPreset};
use pincer_cli::state;
use pincer_cli::ui;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;
use std::time::Instant;

const DEFAULT_PREFETCH_MAX_PAGES: u32 = 20;
const DEFAULT_HN_PROGRESSIVE_INITIAL_COMMENTS: usize = 10;
const DEFAULT_HN_PROGRESSIVE_STEP_COMMENTS: usize = 20;
const ALL_FEEDS: [api::Feed; 4] = [
    api::Feed::Hottest,
    api::Feed::Newest,
    api::Feed::HnTop,
    api::Feed::HnNew,
];

#[derive(Clone, Copy)]
struct RuntimeSettings {
    startup_feed: api::Feed,
    startup_page: u32,
    restore_feed_page: bool,
    high_contrast: bool,
    prefetch_max_pages: u32,
    hn_progressive_initial_comments: usize,
    hn_progressive_step_comments: usize,
    connect_timeout_ms: u64,
    request_timeout_ms: u64,
    retry_attempts: usize,
    retry_backoff_ms: u64,
    hn_comments_fetch_concurrency: usize,
}

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
    batch_complete: bool,
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
    let loaded_config = match config::load_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            app.status = format!("Config load warning: {}", e);
            None
        }
    };
    let (keymap, settings) = resolve_settings(&mut app, loaded_config.as_ref());
    app.high_contrast = settings.high_contrast;
    api::set_runtime_config(api::ApiRuntimeConfig {
        connect_timeout_ms: settings.connect_timeout_ms,
        request_timeout_ms: settings.request_timeout_ms,
        retry_attempts: settings.retry_attempts,
        retry_backoff_ms: settings.retry_backoff_ms,
        hn_comments_fetch_concurrency: settings.hn_comments_fetch_concurrency,
    });
    app.feed = settings.startup_feed;
    app.page = settings.startup_page;
    let mut restored_selection: Option<usize> = None;
    match state::load_state() {
        Ok(Some(saved)) => {
            if settings.restore_feed_page {
                app.feed = saved.feed;
                app.page = saved.page.max(1);
            }
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
        settings,
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
    settings: RuntimeSettings,
) -> Result<()> {
    let (stories_tx, stories_rx) = mpsc::channel::<StoriesLoadResult>();
    let (prefetch_tx, prefetch_rx) = mpsc::channel::<StoriesPrefetchResult>();
    // Seed cache from disk so stale-while-revalidate has data on startup
    for feed in [
        api::Feed::Hottest,
        api::Feed::Newest,
        api::Feed::HnTop,
        api::Feed::HnNew,
    ] {
        if let Some(disk) = cache::load_stories_from_disk(feed, 1) {
            app.cache_stories(feed, 1, disk.stories);
        }
    }
    refresh_stories(
        app,
        &stories_tx,
        &prefetch_tx,
        true,
        settings.prefetch_max_pages,
    );
    let mut pending_restored_selection = restored_selection;
    let mut last_keepalive = Instant::now();

    loop {
        app.tick_frame();
        apply_stories_load_results(app, &stories_rx, &prefetch_tx, settings.prefetch_max_pages);
        // Fire queued refresh now that loading finished
        if app.pending_refresh && !app.stories_loading {
            app.pending_refresh = false;
            if app.nav_mode == NavMode::Infinite {
                app.reset_stories();
            } else {
                app.invalidate_feed_story_cache(app.feed);
            }
            refresh_stories(
                app,
                &stories_tx,
                &prefetch_tx,
                false,
                settings.prefetch_max_pages,
            );
        }
        // Silent keepalive: re-fetch current page every 60s while not loading
        if app.nav_mode != NavMode::Infinite
            && !app.stories_loading
            && !app.stories.is_empty()
            && last_keepalive.elapsed() > Duration::from_secs(60)
        {
            last_keepalive = Instant::now();
            app.invalidate_feed_story_cache(app.feed);
            refresh_stories(
                app,
                &stories_tx,
                &prefetch_tx,
                false,
                settings.prefetch_max_pages,
            );
        }
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
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(
                        app,
                        key.code,
                        &stories_tx,
                        &prefetch_tx,
                        comments_tx,
                        keymap,
                        settings,
                    );
                    // Reload after mode toggle
                    if app.needs_initial_load {
                        app.needs_initial_load = false;
                        refresh_stories(
                            app,
                            &stories_tx,
                            &prefetch_tx,
                            false,
                            settings.prefetch_max_pages,
                        );
                    }
                    // Infinite scroll: preload next page when approaching bottom
                    if app.needs_more_stories() {
                        app.page = app.page.saturating_add(1);
                        refresh_stories(
                            app,
                            &stories_tx,
                            &prefetch_tx,
                            true,
                            settings.prefetch_max_pages,
                        );
                    }
                }
                Event::Resize(_, _) => {
                    // Force redraw on next frame — terminal.draw() picks up new size
                }
                _ => {}
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
    settings: RuntimeSettings,
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
                | KeyAction::ToggleNavMode
        )
        && !(app.nav_mode == NavMode::Infinite
            && matches!(action, KeyAction::MoveDown | KeyAction::MoveUp))
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
        KeyAction::ToggleNavMode => app.toggle_nav_mode(),
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
                if app.stories_loading {
                    app.pending_refresh = true;
                    app.status = String::from("Refresh queued…");
                } else {
                    if app.nav_mode == NavMode::Infinite {
                        app.reset_stories();
                    } else {
                        app.invalidate_feed_story_cache(app.feed);
                    }
                    refresh_stories(
                        app,
                        stories_tx,
                        prefetch_tx,
                        false,
                        settings.prefetch_max_pages,
                    );
                }
            } else {
                refresh_current_comments(app, comments_tx, settings);
            }
        }
        KeyAction::NextPage => {
            if matches!(app.current_view(), View::List) {
                if app.nav_mode == NavMode::Infinite {
                    let jump = 25.min(app.stories.len().saturating_sub(1));
                    let target = (app.selected + jump).min(app.stories.len().saturating_sub(1));
                    app.selected = target;
                    // Preload if we jumped near the bottom
                    if app.selected + 15 >= app.stories.len() && !app.stories_loading {
                        app.page = app.page.saturating_add(1);
                        app.status = String::from("Preloading more stories...");
                        refresh_stories(
                            app,
                            stories_tx,
                            prefetch_tx,
                            true,
                            settings.prefetch_max_pages,
                        );
                    }
                } else {
                    app.next_page();
                    refresh_stories(
                        app,
                        stories_tx,
                        prefetch_tx,
                        true,
                        settings.prefetch_max_pages,
                    );
                }
            }
        }
        KeyAction::PrevPage => {
            if matches!(app.current_view(), View::List) {
                if app.nav_mode == NavMode::Infinite {
                    app.selected = app.selected.saturating_sub(25);
                } else {
                    app.prev_page();
                    refresh_stories(
                        app,
                        stories_tx,
                        prefetch_tx,
                        true,
                        settings.prefetch_max_pages,
                    );
                }
            }
        }
        KeyAction::CycleFeed => {
            if matches!(app.current_view(), View::List) {
                app.reset_stories();
                let next = app.feed.cycle();
                app.feed = next;
                refresh_stories(
                    app,
                    stories_tx,
                    prefetch_tx,
                    true,
                    settings.prefetch_max_pages,
                );
                // Predictive prefetch: also warm the cache for the feed after next
                let next_feed = app.feed.cycle();
                let next_cache = app.cached_stories(next_feed, 1);
                if next_cache.map_or(true, |c| c.is_expired()) {
                    ensure_feed_prefetch(
                        app,
                        next_feed,
                        prefetch_tx,
                        1,
                        settings.prefetch_max_pages,
                    );
                }
            }
        }
        KeyAction::OpenComments => {
            if matches!(app.current_view(), View::List) {
                open_comments(app, comments_tx, settings);
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

fn resolve_settings(
    app: &mut App,
    persisted_config: Option<&config::PersistedConfig>,
) -> (Keymap, RuntimeSettings) {
    let mut preset = KeymapPreset::default();
    let mut startup_feed = api::Feed::Hottest;
    let mut startup_page = 1_u32;
    let mut restore_feed_page = false;
    let mut high_contrast = app.high_contrast;
    let mut prefetch_max_pages = DEFAULT_PREFETCH_MAX_PAGES;
    let mut hn_progressive_initial_comments = DEFAULT_HN_PROGRESSIVE_INITIAL_COMMENTS;
    let mut hn_progressive_step_comments = DEFAULT_HN_PROGRESSIVE_STEP_COMMENTS;
    let mut connect_timeout_ms = 5_000_u64;
    let mut request_timeout_ms = 12_000_u64;
    let mut retry_attempts = 2_usize;
    let mut retry_backoff_ms = 200_u64;
    let mut hn_comments_fetch_concurrency = 12_usize;

    if let Some(cfg) = persisted_config {
        if let Some(config_preset) = cfg.keymap {
            preset = config_preset;
        }
        if let Some(startup) = cfg.startup.as_ref() {
            if let Some(feed) = startup.feed.as_deref() {
                match api::Feed::from_str(feed) {
                    Ok(parsed) => startup_feed = parsed,
                    Err(err) => app.status = format!("Config warning: {}", err),
                }
            }
            if let Some(page) = startup.page {
                startup_page = page.max(1);
            }
            if let Some(restore) = startup.restore_feed_page {
                restore_feed_page = restore;
            }
            if let Some(ref mode_str) = startup.nav_mode {
                match mode_str.to_ascii_lowercase().as_str() {
                    "infinite" => app.nav_mode = NavMode::Infinite,
                    _ => app.nav_mode = NavMode::Paged,
                }
            }
        }
        if let Some(ui) = cfg.ui.as_ref() {
            if let Some(cfg_high_contrast) = ui.high_contrast {
                high_contrast = cfg_high_contrast;
            }
        }
        if let Some(perf) = cfg.performance.as_ref() {
            if let Some(value) = perf.prefetch_max_pages {
                prefetch_max_pages = value.max(1);
            }
            if let Some(value) = perf.hn_progressive_initial_comments {
                hn_progressive_initial_comments = value.max(1);
            }
            if let Some(value) = perf.hn_progressive_step_comments {
                hn_progressive_step_comments = value.max(1);
            }
            if let Some(value) = perf.hn_comments_fetch_concurrency {
                hn_comments_fetch_concurrency = value.max(1);
            }
        }
        if let Some(network) = cfg.network.as_ref() {
            if let Some(value) = network.connect_timeout_ms {
                connect_timeout_ms = value.max(1);
            }
            if let Some(value) = network.request_timeout_ms {
                request_timeout_ms = value.max(1);
            }
            if let Some(value) = network.retry_attempts {
                retry_attempts = value.max(1);
            }
            if let Some(value) = network.retry_backoff_ms {
                retry_backoff_ms = value;
            }
        }
    }

    if let Ok(value) = std::env::var("PINCER_KEYMAP") {
        match value.parse::<KeymapPreset>() {
            Ok(parsed) => preset = parsed,
            Err(err) => app.status = format!("Config warning: {}", err),
        }
    }
    if let Ok(value) = std::env::var("PINCER_STARTUP_FEED") {
        match api::Feed::from_str(&value) {
            Ok(feed) => startup_feed = feed,
            Err(err) => app.status = format!("Config warning: {}", err),
        }
    }
    if let Ok(value) = std::env::var("PINCER_STARTUP_PAGE") {
        match value.parse::<u32>() {
            Ok(page) => startup_page = page.max(1),
            Err(err) => app.status = format!("Config warning: invalid startup page: {}", err),
        }
    }
    if let Some(value) = parse_env_bool("PINCER_STARTUP_RESTORE_FEED_PAGE", app) {
        restore_feed_page = value;
    }
    if let Some(value) = parse_env_bool("PINCER_HIGH_CONTRAST", app) {
        high_contrast = value;
    }
    if let Some(value) = parse_env_u32("PINCER_PREFETCH_MAX_PAGES", app) {
        prefetch_max_pages = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HN_PROGRESSIVE_INITIAL_COMMENTS", app) {
        hn_progressive_initial_comments = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HN_PROGRESSIVE_STEP_COMMENTS", app) {
        hn_progressive_step_comments = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HN_COMMENTS_FETCH_CONCURRENCY", app) {
        hn_comments_fetch_concurrency = value.max(1);
    }
    if let Some(value) = parse_env_u64("PINCER_HTTP_CONNECT_TIMEOUT_MS", app) {
        connect_timeout_ms = value.max(1);
    }
    if let Some(value) = parse_env_u64("PINCER_HTTP_REQUEST_TIMEOUT_MS", app) {
        request_timeout_ms = value.max(1);
    }
    if let Some(value) = parse_env_usize("PINCER_HTTP_RETRY_ATTEMPTS", app) {
        retry_attempts = value.max(1);
    }
    if let Some(value) = parse_env_u64("PINCER_HTTP_RETRY_BACKOFF_MS", app) {
        retry_backoff_ms = value;
    }

    let keymap = Keymap::new(preset);
    if keymap.preset() != KeymapPreset::Vim {
        app.status = format!("Loaded {} keymap preset", keymap.preset().as_str());
    }
    (
        keymap,
        RuntimeSettings {
            startup_feed,
            startup_page,
            restore_feed_page,
            high_contrast,
            prefetch_max_pages,
            hn_progressive_initial_comments,
            hn_progressive_step_comments,
            connect_timeout_ms,
            request_timeout_ms,
            retry_attempts,
            retry_backoff_ms,
            hn_comments_fetch_concurrency,
        },
    )
}

fn parse_env_bool(var_name: &str, app: &mut App) -> Option<bool> {
    let value = std::env::var(var_name).ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => {
            app.status = format!("Config warning: invalid boolean for {}", var_name);
            None
        }
    }
}

fn parse_env_u32(var_name: &str, app: &mut App) -> Option<u32> {
    let value = std::env::var(var_name).ok()?;
    match value.parse::<u32>() {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            app.status = format!("Config warning: invalid {}: {}", var_name, err);
            None
        }
    }
}

fn parse_env_u64(var_name: &str, app: &mut App) -> Option<u64> {
    let value = std::env::var(var_name).ok()?;
    match value.parse::<u64>() {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            app.status = format!("Config warning: invalid {}: {}", var_name, err);
            None
        }
    }
}

fn parse_env_usize(var_name: &str, app: &mut App) -> Option<usize> {
    let value = std::env::var(var_name).ok()?;
    match value.parse::<usize>() {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            app.status = format!("Config warning: invalid {}: {}", var_name, err);
            None
        }
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
                        match loaded.stage {
                            CommentsLoadStage::Partial => {
                                app.load_comments_partial(detail);
                                app.status = format!(
                                    "{} comments loaded in {}ms (loading more...)",
                                    comments_len, loaded.elapsed_ms
                                );
                            }
                            CommentsLoadStage::Final => {
                                app.cache_story_detail(loaded.short_id.clone(), detail.clone());
                                cache::save_comments_to_disk(&loaded.short_id, &detail);
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

fn refresh_current_comments(
    app: &mut App,
    comments_tx: &Sender<CommentsLoadResult>,
    settings: RuntimeSettings,
) {
    let Some(story) = app.selected_story() else {
        app.status = String::from("No story selected");
        return;
    };
    let short_id = story.short_id.clone();
    app.invalidate_story_cache(&short_id);
    open_comments(app, comments_tx, settings);
}

fn refresh_stories(
    app: &mut App,
    stories_tx: &Sender<StoriesLoadResult>,
    prefetch_tx: &Sender<StoriesPrefetchResult>,
    use_cache: bool,
    prefetch_max_pages: u32,
) {
    let count = if app.nav_mode == NavMode::Infinite && app.stories.is_empty() {
        if app.feed.source() == api::Source::HackerNews {
            1 // HN needs 25 individual API calls per page — just load page 1
        } else {
            4
        }
    } else {
        1
    };

    // Stale-while-revalidate: serve cached stories immediately if usable,
    // then revalidate in background.
    if use_cache && count == 1 {
        if let Some(cached) = app.cached_stories(app.feed, app.page) {
            if cached.is_fresh() {
                // Fresh cache: serve and done
                if app.nav_mode == NavMode::Infinite {
                    app.append_stories(cached.stories);
                } else {
                    app.stories = cached.stories;
                    app.selected = 0;
                    app.status = format!("Loaded {} stories (cached)", app.stories.len());
                }
                ensure_feed_prefetch(app, app.feed, prefetch_tx, 2, prefetch_max_pages);
                return;
            }
            if cached.is_stale_but_usable() {
                // Stale cache: serve now, refresh silently in background
                if app.nav_mode == NavMode::Infinite {
                    app.append_stories(cached.stories);
                } else {
                    app.stories = cached.stories;
                    app.selected = 0;
                    app.status = format!("Loaded {} stories (cached)", app.stories.len());
                }
                // Fall through to background refresh
            }
        }
    }
    // Batch loads: progressive loading shows pages as they arrive, no cache needed.

    if app.stories_loading && count == 1 {
        // Still loading from a previous fetch — don't stack threads
        return;
    }

    let request_id = if use_cache
        && count == 1
        && app
            .cached_stories(app.feed, app.page)
            .map_or(false, |c| c.is_stale_but_usable())
    {
        // Silent background refresh
        app.background_refreshing = true;
        app.allocate_request_id()
    } else {
        app.begin_stories_loading()
    };
    let feed = app.feed;
    let start_page = app.page;
    let sender = stories_tx.clone();
    thread::spawn(move || {
        if count <= 1 {
            // Single page: simple fetch
            let (resolved_page, fell_back_to_first_page, result) =
                fetch_stories_with_fallback(feed, start_page);
            let _ = sender.send(StoriesLoadResult {
                request_id,
                feed,
                requested_page: start_page,
                resolved_page,
                fell_back_to_first_page,
                result: result.map_err(|e| e.to_string()),
                batch_complete: true,
            });
        } else {
            // Batch: fetch all pages — use the batch API (HN fetches IDs once)
            let results = api::fetch_stories_batch(feed, start_page, count);
            let total = results.len();
            let mut had_data = false;
            for (i, result) in results.into_iter().enumerate() {
                let page = start_page + i as u32;
                let is_last = i + 1 == total;
                match result {
                    Ok(stories) => {
                        had_data = true;
                        let _ = sender.send(StoriesLoadResult {
                            request_id,
                            feed,
                            requested_page: page,
                            resolved_page: page,
                            fell_back_to_first_page: false,
                            result: Ok(stories),
                            batch_complete: is_last,
                        });
                    }
                    Err(e) => {
                        if !had_data {
                            let _ = sender.send(StoriesLoadResult {
                                request_id,
                                feed,
                                requested_page: start_page,
                                resolved_page: page,
                                fell_back_to_first_page: false,
                                result: Err(e.to_string()),
                                batch_complete: true,
                            });
                        }
                        break;
                    }
                }
            }
        }
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
    prefetch_max_pages: u32,
) {
    loop {
        match stories_rx.try_recv() {
            Ok(loaded) => {
                if !app.is_current_stories_request(loaded.request_id) {
                    continue;
                }
                if loaded.batch_complete {
                    app.finish_stories_loading();
                    app.background_refreshing = false;
                }
                match loaded.result {
                    Ok(stories) => {
                        app.cache_stories(loaded.feed, loaded.resolved_page, stories.clone());
                        cache::save_stories_to_disk(loaded.feed, loaded.resolved_page, &stories);
                        if app.nav_mode == NavMode::Infinite {
                            // Infinite mode: always append — one seamless list, no page concept
                            // Stories are already cleared by reset_stories() on feed switch
                            if !app.stories.is_empty() && loaded.feed != app.feed {
                                app.stories.clear();
                                app.selected = 0;
                            }
                            app.page = loaded.resolved_page;
                            app.append_stories(stories);
                        } else {
                            app.page = loaded.resolved_page;
                            app.stories = stories;
                            app.selected = 0;
                            ensure_feed_prefetch(
                                app,
                                loaded.feed,
                                prefetch_tx,
                                2,
                                prefetch_max_pages,
                            );
                            for feed in ALL_FEEDS {
                                if feed != loaded.feed {
                                    ensure_feed_prefetch(
                                        app,
                                        feed,
                                        prefetch_tx,
                                        1,
                                        prefetch_max_pages,
                                    );
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
                    }
                    Err(error_message) => {
                        if app.nav_mode != NavMode::Infinite {
                            app.status = format!("Error fetching stories: {}", error_message);
                        }
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
    prefetch_max_pages: u32,
) {
    if !app.begin_feed_prefetch(feed) {
        return;
    }

    let sender = prefetch_tx.clone();
    thread::spawn(move || {
        for page in start_page..=prefetch_max_pages.max(start_page) {
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

fn open_comments(
    app: &mut App,
    comments_tx: &Sender<CommentsLoadResult>,
    settings: RuntimeSettings,
) {
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
            let final_result = api::fetch_story_detail_progressive(
                app_feed,
                &short_id,
                settings.hn_progressive_initial_comments,
                settings.hn_progressive_step_comments,
                |partial| {
                    let _ = sender.send(CommentsLoadResult {
                        short_id: short_id.clone(),
                        stage: CommentsLoadStage::Partial,
                        result: Ok(partial),
                        elapsed_ms: started.elapsed().as_millis(),
                    });
                },
            )
            .map_err(|e| e.to_string());

            let _ = sender.send(CommentsLoadResult {
                short_id,
                stage: CommentsLoadStage::Final,
                result: final_result,
                elapsed_ms: started.elapsed().as_millis(),
            });
            return;
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
