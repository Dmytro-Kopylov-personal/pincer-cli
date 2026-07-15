// hermes-verify: ad-hoc verification for comment-selection highlight change.
// Not part of the permanent suite intent — exercises ui::draw_comments via a
// TestBackend to confirm (a) no panic across empty/populated/deleted comment
// cases and varying depths, and (b) the selected row actually renders with a
// distinct background from unselected rows (the actual behavior requested).
use pincer_cli::api::{Comment, Feed};
use pincer_cli::app::{App, View};
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
    app.view = View::Comments;
    app
}

#[test]
fn empty_comments_does_not_panic() {
    let app = base_app();
    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &app)).unwrap();
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
    term.draw(|f| pincer_cli::ui::draw(f, &app)).unwrap();

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
                return Some(buf[(x, y)].style().bg.unwrap_or(ratatui::style::Color::Reset));
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
    term.draw(|f| pincer_cli::ui::draw(f, &app)).unwrap();
}
