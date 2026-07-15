use pincer_cli::api::Comment;
use pincer_cli::app::{App, View};
use ratatui::{backend::TestBackend, Terminal};

fn mk_long_comment(short_id: &str, depth: usize) -> Comment {
    Comment {
        short_id: short_id.to_string(),
        comment_plain: "word ".repeat(200),
        score: 3,
        depth,
        commenting_user: format!("user_{short_id}"),
        is_deleted: false,
    }
}

#[test]
fn long_comment_body_wraps_and_does_not_overflow_narrow_terminal() {
    let mut app = App::new();
    app.view = View::Comments;
    app.comments = vec![mk_long_comment("a1", 0), mk_long_comment("a2", 3)];
    app.comment_selected = 0;

    let width = 40u16;
    let backend = TestBackend::new(width, 20);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &app)).unwrap();

    let buf = term.backend().buffer();
    let border_col = width - 1;
    let mut saw_border_glyph = false;
    for y in 0..buf.area().height {
        let sym = buf[(border_col, y)].symbol();
        if sym == "\u{2502}" || sym == "\u{2510}" || sym == "\u{2518}" {
            saw_border_glyph = true;
        }
    }
    assert!(
        saw_border_glyph,
        "right border of comments box must remain intact (not overwritten by unwrapped text)"
    );
}

#[test]
fn scrolling_far_past_first_screen_does_not_panic() {
    let mut app = App::new();
    app.view = View::Comments;
    app.comments = (0..200)
        .map(|i| mk_long_comment(&format!("c{i}"), i % 5))
        .collect();
    app.comment_selected = 150;

    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, &app)).unwrap();
}
