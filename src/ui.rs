use crate::app::{App, View};
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

const MAX_THREAD_INDENT_LEVEL: usize = 6;

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(size);

    match app.view {
        View::List => draw_list(f, app, chunks[0]),
        View::Comments => draw_comments(f, app, chunks[0]),
    }

    draw_status(f, app, chunks[1]);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(" claw — {} (page {}) ", app.feed.label(), app.page);
    let items: Vec<ListItem> = app
        .stories
        .iter()
        .map(|s| {
            let line = Line::from(vec![
                Span::styled(
                    format!("{:>4} ", s.score),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(s.title.clone(), Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", s.tags.join(",")),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("({}c)", s.comment_count),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("by {}", s.submitter_user),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_comments(f: &mut Frame, app: &App, area: Rect) {
    let title = if !app.story_detail_title.is_empty() {
        format!(" {} ", app.story_detail_title)
    } else {
        app.selected_story()
            .map(|s| format!(" {} ", s.title))
            .unwrap_or_else(|| " comments ".to_string())
    };

    // Inner width available for text once borders are subtracted; used to
    // hard-wrap comment bodies so long lines can't overflow past the right
    // border and corrupt the box-drawing frame.
    let has_scrollbar = !app.comments.is_empty();
    let inner_width = area
        .width
        .saturating_sub(2 + if has_scrollbar { 1 } else { 0 })
        .max(1) as usize;

    let items: Vec<ListItem> = app
        .comments
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let depth_indent = "  ".repeat(c.depth.min(MAX_THREAD_INDENT_LEVEL));
            let depth_prefix = if c.depth == 0 { "" } else { "↳ " };
            let selected = i == app.comment_selected;
            let marker = if selected { "▶ " } else { "  " };
            let row_bg = if selected {
                Some(Color::Rgb(40, 40, 40))
            } else {
                None
            };
            let with_row_bg = |mut s: Style| -> Style {
                if let Some(bg) = row_bg {
                    s = s.bg(bg);
                }
                s
            };
            let user_style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                with_row_bg(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            };
            let header = Line::from(vec![
                Span::styled(
                    marker,
                    with_row_bg(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                ),
                Span::styled(depth_indent.clone(), with_row_bg(Style::default())),
                Span::styled(depth_prefix, with_row_bg(Style::default().fg(Color::DarkGray))),
                Span::styled(format!(" {} ", c.commenting_user), user_style),
                Span::styled(" ", with_row_bg(Style::default())),
                Span::styled(
                    format!("({})", c.score),
                    with_row_bg(Style::default().fg(Color::Yellow)),
                ),
            ]);
            let body = if c.is_deleted {
                "[deleted]".to_string()
            } else {
                c.comment_plain.trim().to_string()
            };
            let body_style = if selected {
                with_row_bg(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Style::default().fg(Color::Gray)
            };
            let body_indent = format!("{}  ", depth_indent);
            let wrap_width = inner_width.saturating_sub(body_indent.chars().count()).max(1);
            let body_lines = wrap_comment_body(&body, &body_indent, body_style, wrap_width);

            let mut lines = vec![header];
            lines.extend(body_lines);
            lines.push(Line::from(""));
            if selected {
                for line in lines.iter_mut() {
                    line.style = line.style.bg(Color::Rgb(40, 40, 40));
                }
            }
            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    let mut state = ListState::default();
    state.select(Some(app.comment_selected));
    f.render_stateful_widget(list, area, &mut state);

    if has_scrollbar {
        let mut scrollbar_state = ScrollbarState::new(app.comments.len())
            .position(app.comment_selected)
            .viewport_content_length(area.height.saturating_sub(2).max(1) as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn wrap_comment_body(
    body: &str,
    body_indent: &str,
    body_style: Style,
    wrap_width: usize,
) -> Vec<Line<'static>> {
    let clean = body.replace('\r', "");
    let wrap_options = textwrap::Options::new(wrap_width)
        .break_words(true)
        .word_splitter(textwrap::WordSplitter::NoHyphenation);

    let mut lines = Vec::new();
    for paragraph in clean.lines() {
        let normalized = paragraph.replace('\t', "    ");
        let trimmed = normalized.trim_end();
        if trimmed.is_empty() {
            lines.push(Line::from(Span::styled(body_indent.to_string(), body_style)));
            continue;
        }
        for wrapped in textwrap::wrap(trimmed, &wrap_options) {
            lines.push(Line::from(Span::styled(
                format!("{}{}", body_indent, wrapped),
                body_style,
            )));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(body_indent.to_string(), body_style)));
    }
    lines
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.view {
        View::List => {
            "j/k move  enter=comments  o=open link  tab=switch feed  pgup/pgdn=page  r=refresh  q=quit"
        }
        View::Comments => {
            "j/k move  o=open story  c=open comment permalink  b=open comments in browser  esc=back  q=quit"
        }
    };
    let text = format!("{}\n{}", app.status, help);
    let p = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}
