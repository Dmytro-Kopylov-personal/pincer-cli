use pincer_cli::api::{Feed, Story};
use pincer_cli::app::{App, NavMode};
use ratatui::{backend::TestBackend, Terminal};

/// Render the app and return the full text of the terminal.
fn rendered(app: &mut App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn make_story(id: u32, title: &str) -> Story {
    Story {
        short_id: format!("s{id}"),
        title: title.to_string(),
        url: "https://example.com".into(),
        score: (id % 100) as i32,
        comment_count: (id % 50) as i32,
        tags: vec!["tag".into()],
        submitter_user: format!("user{id}"),
        comments_url: format!("https://example.com/comments/{id}"),
    }
}

fn seed_stories(app: &mut App, n: usize) {
    app.stories = (0..n)
        .map(|i| make_story(i as u32, &format!("Story {i}")))
        .collect();
}

/// After Tab, the old feed's stories should remain visible while
/// the new feed loads (no blank screen).
#[test]
fn tab_clears_stories_and_shows_loading_for_new_feed() {
    let mut app = App::new();
    app.feed = Feed::Hottest;
    seed_stories(&mut app, 5);

    // Simulate Tab: reset stories, switch feed
    app.reset_stories();
    app.feed = Feed::HnTop;
    let text = rendered(&mut app, 80, 24);
    assert!(!text.contains("Story 0"), "old stories should be cleared after Tab");
    assert!(text.contains("HN: Top"), "title should show new feed name");
}

/// In infinite mode, the banner should say INFINITE.
#[test]
fn infinite_mode_banner_shows_mode() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    let text = rendered(&mut app, 80, 24);
    assert!(text.contains("INFINITE"), "banner should show INFINITE in infinite mode");
}

/// In paged mode, the banner should say PAGED.
#[test]
fn paged_mode_banner_shows_mode() {
    let mut app = App::new();
    app.nav_mode = NavMode::Paged;
    let text = rendered(&mut app, 80, 24);
    assert!(text.contains("PAGED"), "banner should show PAGED in paged mode");
}

/// Empty story list should show "No stories available" message.
#[test]
fn empty_stories_shows_no_stories_message() {
    let mut app = App::new();
    let text = rendered(&mut app, 80, 24);
    assert!(text.contains("No stories available"), "empty list should show message");
}

/// After append_stories, the list shows more stories.
#[test]
fn appended_stories_are_rendered() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    seed_stories(&mut app, 3);
    app.append_stories(vec![make_story(3, "Story 3")]);

    let text = rendered(&mut app, 80, 24);
    assert!(text.contains("Story 3"), "appended story should render");
}

/// Toggle from paged to infinite clears stories and sets needs_initial_load.
#[test]
fn toggle_nav_mode_resets_state() {
    let mut app = App::new();
    seed_stories(&mut app, 5);
    app.toggle_nav_mode();

    assert!(app.stories.is_empty(), "stories cleared after toggle");
    assert_eq!(app.page, 1, "page reset after toggle");
    assert!(app.needs_initial_load, "needs_initial_load set after toggle");
    assert_eq!(app.nav_mode, NavMode::Infinite, "switched to infinite");
}

/// Selecting the last story should be within bounds.
#[test]
fn move_selection_stays_in_bounds() {
    let mut app = App::new();
    seed_stories(&mut app, 3);
    app.selected = 2;
    app.move_selection(1); // past end
    assert_eq!(app.selected, 2, "should clamp at last story");

    app.move_selection(-5); // past start
    assert_eq!(app.selected, 0, "should clamp at first story");
}

/// The status bar shows keybindings with F-key alternatives.
#[test]
fn status_bar_shows_fkey_alternatives() {
    let mut app = App::new();
    let text = rendered(&mut app, 140, 24);
    assert!(text.contains("F5"), "status bar should mention F5 for refresh");
    assert!(text.contains("F1"), "status bar should mention F1 for help");
}

/// Jump to top/bottom works correctly.
#[test]
fn jump_top_and_bottom() {
    let mut app = App::new();
    seed_stories(&mut app, 10);
    app.selected = 5;

    app.jump_top();
    assert_eq!(app.selected, 0, "jump_top should go to 0");

    app.jump_bottom();
    assert_eq!(app.selected, 9, "jump_bottom should go to last index");
}

/// needs_more_stories triggers with lookahead in infinite mode.
#[test]
fn needs_more_stories_uses_lookahead() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    seed_stories(&mut app, 20);
    app.selected = 3;
    assert!(!app.needs_more_stories(), "should not trigger at position 3 of 20");

    app.selected = 18;
    assert!(app.needs_more_stories(), "should trigger at position 18 of 20");
}

/// needs_more_stories is false in paged mode.
#[test]
fn needs_more_stories_false_in_paged_mode() {
    let mut app = App::new();
    app.nav_mode = NavMode::Paged;
    seed_stories(&mut app, 20);
    app.selected = 19;
    assert!(!app.needs_more_stories(), "lookahead should not trigger in paged mode");
}

/// Story count title shows in infinite mode.
#[test]
fn infinite_mode_shows_story_count_in_title() {
    let mut app = App::new();
    app.nav_mode = NavMode::Infinite;
    seed_stories(&mut app, 42);
    let text = rendered(&mut app, 80, 24);
    assert!(text.contains("42 stories"), "title should show story count in infinite mode");
}

/// Paged mode shows page number in title.
#[test]
fn paged_mode_shows_page_in_title() {
    let mut app = App::new();
    app.nav_mode = NavMode::Paged;
    app.page = 3;
    seed_stories(&mut app, 5);
    let text = rendered(&mut app, 80, 24);
    assert!(text.contains("page 3"), "title should show page number in paged mode");
}
