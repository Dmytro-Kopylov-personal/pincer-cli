use pincer_cli::api::{Comment, Feed};
use pincer_cli::app::App;
use ratatui::{backend::TestBackend, Terminal};

fn mk_comment(short_id: &str, depth: usize, deleted: bool) -> Comment {
    Comment {
        short_id: short_id.to_string(),
        comment_plain: format!("body of {short_id}"),
        score: 3,
        depth,
        commenting_user: format!("user_{short_id}"),
        is_deleted: deleted,
    }
}

fn base_app() -> App {
    let mut app = App::new();
    app.feed = Feed::Hottest;
    app.enter_comments_view();
    app
}

#[test]
fn empty_comments_does_not_panic() {
    let mut app = base_app();
    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &mut app)).unwrap();
}

#[test]
fn loading_state_renders_loading_message() {
    let mut app = base_app();
    app.comments_loading = true;
    app.story_detail_title = "Loading story".to_string();

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &mut app)).unwrap();

    let buf = term.backend().buffer().clone();
    let mut rendered = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            rendered.push_str(buf[(x, y)].symbol());
        }
        rendered.push('\n');
    }
    assert!(
        rendered.contains("Loading comments..."),
        "comments view should display loading text while comments fetch is in progress"
    );
}

#[test]
fn populated_comments_render_without_panic_and_highlight_selected() {
    let mut app = base_app();
    app.comments = vec![
        mk_comment("a1", 0, false),
        mk_comment("a2", 1, false),
        mk_comment("a3", 2, true), // deleted
    ];
    app.comment_selected = 1;

    let backend = TestBackend::new(100, 30);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &mut app)).unwrap();

    let buf = term.backend().buffer().clone();
    // Selected comment (index 1 -> "user_a2") should have a visibly different
    // bg than an unselected comment's row (index 0 -> "user_a1").
    let find_row_bg = |needle: &str| -> Option<ratatui::style::Color> {
        for y in 0..buf.area().height {
            let mut line = String::new();
            for x in 0..buf.area().width {
                line.push_str(buf[(x, y)].symbol());
            }
            if let Some(byte_idx) = line.find(needle) {
                let x = line[..byte_idx].chars().count() as u16;
                return Some(
                    buf[(x, y)]
                        .style()
                        .bg
                        .unwrap_or(ratatui::style::Color::Reset),
                );
            }
        }
        None
    };

    let selected_bg = find_row_bg("user_a2").expect("selected user row must render");
    let unselected_bg = find_row_bg("user_a1").expect("unselected user row must render");
    assert_ne!(
        selected_bg, unselected_bg,
        "selected comment row must have a visually distinct background from unselected rows"
    );
}

#[test]
fn selection_bounds_at_last_comment_does_not_panic() {
    let mut app = base_app();
    app.comments = vec![mk_comment("only", 0, false)];
    app.comment_selected = 0;
    let backend = TestBackend::new(60, 20);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &mut app)).unwrap();
}

#[test]
fn nested_comment_text_stays_compact_on_the_left() {
    let mut app = base_app();
    app.comments = vec![mk_comment("root", 0, false), mk_comment("deep", 5, false)];
    app.comment_selected = 0;

    let backend = TestBackend::new(100, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &mut app)).unwrap();

    let buf = term.backend().buffer().clone();
    let find_x = |needle: &str| -> Option<u16> {
        for y in 0..buf.area().height {
            let mut line = String::new();
            for x in 0..buf.area().width {
                line.push_str(buf[(x, y)].symbol());
            }
            if let Some(byte_idx) = line.find(needle) {
                return Some(line[..byte_idx].chars().count() as u16);
            }
        }
        None
    };

    let root_x = find_x("user_root").expect("root comment must render");
    let deep_x = find_x("user_deep").expect("deep comment must render");
    assert!(
        deep_x.saturating_sub(root_x) <= 14,
        "deeply nested comments should use capped indentation and stay readable"
    );
}
