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

pub fn draw(f: &mut Frame, app: &mut App) {
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
    if app.show_help {
        draw_help_overlay(f);
    }
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

fn draw_comments(f: &mut Frame, app: &mut App, area: Rect) {
    let title = if !app.story_detail_title.is_empty() {
        format!(" {} ", app.story_detail_title)
    } else {
        app.selected_story()
            .map(|s| format!(" {} ", s.title))
            .unwrap_or_else(|| " comments ".to_string())
    };

    if app.comments_loading && app.comments.is_empty() {
        let loading = Paragraph::new("Loading comments...")
            .block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(loading, area);
        return;
    }

    // Inner width available for text once borders are subtracted; used to
    // hard-wrap comment bodies so long lines can't overflow past the right
    // border and corrupt the box-drawing frame.
    let has_scrollbar = !app.comments.is_empty();
    let inner_width = area
        .width
        .saturating_sub(2 + if has_scrollbar { 1 } else { 0 })
        .max(1) as usize;
    app.ensure_wrapped_comments(inner_width, MAX_THREAD_INDENT_LEVEL);

    let display_indices = app.comment_indices_for_display();
    let items: Vec<ListItem> = display_indices
        .iter()
        .copied()
        .map(|actual_index| {
            let c = &app.comments[actual_index];
            let depth_indent = "  ".repeat(c.depth.min(MAX_THREAD_INDENT_LEVEL));
            let depth_prefix = if c.depth == 0 { "" } else { "↳ " };
            let selected = actual_index == app.comment_selected;
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
                Span::styled(
                    depth_prefix,
                    with_row_bg(Style::default().fg(Color::DarkGray)),
                ),
                Span::styled(format!(" {} ", c.commenting_user), user_style),
                Span::styled(" ", with_row_bg(Style::default())),
                Span::styled(
                    format!("({})", c.score),
                    with_row_bg(Style::default().fg(Color::Yellow)),
                ),
            ]);
            let body_style = if selected {
                with_row_bg(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Style::default().fg(Color::Gray)
            };
            let body_lines: Vec<Line> = app
                .wrapped_comment_lines(actual_index)
                .map(|lines| {
                    lines
                        .iter()
                        .map(|line| Line::from(Span::styled(line.clone(), body_style)))
                        .collect()
                })
                .unwrap_or_default();
            let body_lines = if app.is_comment_collapsed(actual_index) {
                vec![Line::from(Span::styled(
                    format!("{}  [collapsed]", depth_indent),
                    body_style.fg(Color::DarkGray),
                ))]
            } else {
                body_lines
            };

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
    state.select(app.comment_display_position(app.comment_selected));
    f.render_stateful_widget(list, area, &mut state);

    if has_scrollbar && !display_indices.is_empty() {
        let mut scrollbar_state = ScrollbarState::new(display_indices.len())
            .position(
                app.comment_display_position(app.comment_selected)
                    .unwrap_or(0),
            )
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

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let mut help = match app.view {
        View::List => {
            "j/k move  enter=comments  o=open link  tab=switch feed  pgup/pgdn=page  r=refresh  ?=help  p=prof  q=quit"
        }
        View::Comments => {
            "j/k move  /=search  n=next  H=high-score  z=collapse  c=comment link  b=open thread  ?=help  p=prof  esc=back"
        }
    }
    .to_string();
    if app.search_mode {
        help = format!("SEARCH: {} (Enter apply, Esc cancel)", app.search_input);
    }
    let mut status = app.status.clone();
    if app.profiling_enabled {
        let last_load = app
            .last_comments_load_ms
            .map(|v| format!("{v}ms"))
            .unwrap_or_else(|| "n/a".to_string());
        status = format!(
            "{status} | frames={} last_load={last_load}",
            app.frame_count
        );
    }
    let text = format!("{}\n{}", status, help);
    let p = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

fn draw_help_overlay(f: &mut Frame) {
    let area = centered_rect(80, 70, f.area());
    let help = "Help\n\nList: j/k, g/G, tab, r, [ ], PageUp/PageDown, enter\nComments: j/k, g/G, / search, n next match, H high-score, z collapse, c permalink\nGlobal: o open story, b open thread, p profiling, ? help, q quit";
    let panel = Paragraph::new(help).block(
        Block::default()
            .title(" Keybindings ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black).fg(Color::White)),
    );
    f.render_widget(panel, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
