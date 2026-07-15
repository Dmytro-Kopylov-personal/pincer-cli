use crate::app::{App, View};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

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

    let items: Vec<ListItem> = app
        .comments
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let indent = "  ".repeat(c.depth);
            let selected = i == app.comment_selected;
            let marker = if selected { "▶ " } else { "  " };
            let row_bg = if selected { Some(Color::Rgb(40, 40, 40)) } else { None };
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
                    with_row_bg(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ),
                Span::styled(indent.clone(), with_row_bg(Style::default())),
                Span::styled(format!(" {} ", c.commenting_user), user_style),
                Span::styled(" ", with_row_bg(Style::default())),
                Span::styled(
                    format!("({})", c.score),
                    with_row_bg(Style::default().fg(Color::Yellow)),
                ),
            ]);
            let body_indent = "  ".repeat(c.depth + 1);
            let body = if c.is_deleted {
                "[deleted]".to_string()
            } else {
                c.comment_plain.trim().to_string()
            };
            let body_style = if selected {
                with_row_bg(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
            } else {
                Style::default().fg(Color::Gray)
            };
            let body_line = Line::from(vec![Span::styled(
                format!("{}{}", body_indent, body),
                body_style,
            )]);
            let mut lines = vec![header, body_line, Line::from("")];
            if selected {
                for line in lines.iter_mut() {
                    line.style = line.style.bg(Color::Rgb(40, 40, 40));
                }
            }
            ListItem::new(lines)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.view {
        View::List => "j/k move  enter=comments  o=open link  tab=switch feed  r=refresh  q=quit",
        View::Comments => "j/k move  o=open link  b=open comments in browser  esc=back  q=quit",
    };
    let text = format!("{}\n{}", app.status, help);
    let p = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}
