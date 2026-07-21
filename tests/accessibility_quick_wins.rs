use pincer_cli::api::{Comment, Feed};
use pincer_cli::app::App;
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| pincer_cli::ui::draw(f, app)).unwrap();
    term.backend().buffer().clone()
}

fn rendered_text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area().height {
        for x in 0..buf.area().width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn line_containing(buf: &Buffer, needle: &str) -> Option<String> {
    for y in 0..buf.area().height {
        let mut line = String::new();
        for x in 0..buf.area().width {
            line.push_str(buf[(x, y)].symbol());
        }
        if line.contains(needle) {
            return Some(line);
        }
    }
    None
}

fn mk_comment(user: &str, id: &str) -> Comment {
    Comment {
        short_id: id.to_string(),
        comment_plain: format!("comment from {user}"),
        score: 1,
        depth: 0,
        commenting_user: user.to_string(),
        is_deleted: false,
    }
}

#[test]
fn search_mode_indicator_is_visible_in_status_area() {
    let mut app = App::new();
    app.enter_comments_view();
    app.start_search_mode();
    app.search_input = "rust".to_string();

    let text = rendered_text(&render(&mut app, 120, 30));
    assert!(text.contains("SEARCH: rust (Enter apply, Esc cancel)"));
}

#[test]
fn help_overlay_displays_keybindings_indicator() {
    let mut app = App::new();
    app.toggle_help();

    let text = rendered_text(&render(&mut app, 120, 35));
    assert!(text.contains("Keybindings"));
    assert!(text.contains("Help"));
}

#[test]
fn selected_comment_has_symbol_marker_not_just_color() {
    let mut app = App::new();
    app.enter_comments_view();
    app.comments = vec![mk_comment("alice", "c1"), mk_comment("bob", "c2")];
    app.comment_selected = 1;

    let buf = render(&mut app, 100, 30);
    let selected_line = line_containing(&buf, " bob ").expect("selected row should render");
    let unselected_line = line_containing(&buf, " alice ").expect("unselected row should render");

    assert!(
        selected_line.contains("▶"),
        "selected row should include shape marker"
    );
    assert!(
        !unselected_line.contains("▶"),
        "unselected row should not include selected marker"
    );
}

#[test]
fn help_line_includes_multiple_keymap_paths() {
    let mut app = App::new();
    // default list state

    let text = rendered_text(&render(&mut app, 140, 24));
    assert!(text.contains("j/k move"));
    assert!(text.contains("pgup/pgdn=page"));
    assert!(text.contains("?=help"));
}

#[test]
fn source_indicators_are_textual_not_color_only() {
    let mut lobsters_app = App::new();
    let lobsters = rendered_text(&render(&mut lobsters_app, 120, 24));
    assert!(lobsters.contains("[L] LOBSTERS"));
    assert!(lobsters.contains("[SRC:L]"));

    let mut hn_app = App::new();
    hn_app.feed = Feed::HnTop;
    let hn = rendered_text(&render(&mut hn_app, 120, 24));
    assert!(hn.contains("[H] HN"));
    assert!(hn.contains("[SRC:H]"));
}
