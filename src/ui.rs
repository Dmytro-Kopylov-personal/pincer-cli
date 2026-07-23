use crate::api::Source;
use crate::app::{App, View};
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

const MAX_THREAD_INDENT_LEVEL: usize = 6;

struct Palette {
    text: Color,
    muted: Color,
    accent: Color,
    warning: Color,
    tag: Color,
    selected_bg: Color,
    selected_user_fg: Color,
    selected_user_bg: Color,
    banner_bg: Color,
    banner_fg: Color,
}

impl Palette {
    fn from_app(app: &App) -> Self {
        if app.high_contrast {
            Self {
                text: Color::White,
                muted: Color::Gray,
                accent: Color::Cyan,
                warning: Color::Yellow,
                tag: Color::LightCyan,
                selected_bg: Color::Blue,
                selected_user_fg: Color::Black,
                selected_user_bg: Color::White,
                banner_bg: Color::White,
                banner_fg: Color::Black,
            }
        } else {
            Self {
                text: Color::White,
                muted: Color::Gray,
                accent: Color::Cyan,
                warning: Color::Yellow,
                tag: Color::Cyan,
                selected_bg: Color::Rgb(40, 40, 40),
                selected_user_fg: Color::Black,
                selected_user_bg: Color::Yellow,
                banner_bg: Color::DarkGray,
                banner_fg: Color::White,
            }
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let palette = Palette::from_app(app);
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(size);

    draw_mode_banner(f, app, chunks[0], &palette);
    match app.current_view() {
        View::List => draw_list(f, app, chunks[1], &palette),
        View::Comments => draw_comments(f, app, chunks[1], &palette),
    }

    draw_status(f, app, chunks[2], &palette);
    if app.is_help_visible() {
        draw_help_overlay(f, &palette);
    }
}

fn draw_mode_banner(f: &mut Frame, app: &App, area: Rect, palette: &Palette) {
    let base_style = Style::default()
        .fg(palette.banner_fg)
        .bg(palette.banner_bg)
        .add_modifier(Modifier::BOLD);
    let source_chip = match app.feed.source() {
        Source::Lobsters => Span::styled(
            " [L] LOBSTERS ",
            if app.high_contrast {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            },
        ),
        Source::HackerNews => Span::styled(
            " [H] HN ",
            if app.high_contrast {
                Style::default().fg(Color::Black).bg(Color::White)
            } else {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            },
        ),
    };
    let banner_line = Line::from(vec![
        Span::styled(app.mode_banner_text(), base_style),
        Span::styled(" SOURCE ", base_style),
        source_chip,
    ]);
    let banner = Paragraph::new(banner_line).style(base_style);
    f.render_widget(banner, area);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect, palette: &Palette) {
    let title = format!(" claw — {} (page {}) ", app.feed.label(), app.page);
    if app.stories.is_empty() {
        let empty = Paragraph::new("No stories available. Press r to refresh.")
            .style(Style::default().fg(palette.muted))
            .block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .stories
        .iter()
        .map(|s| {
            let inner_width = area.width.saturating_sub(2) as usize; // borders only
            let score_str = format!("{:>3} ", s.score);
            let tags_str = format!("[{}]", s.tags.join(","));

            // Priority: always show score + title.
            // Then add extras only if room.
            let mut spans: Vec<Span> = vec![
                Span::styled(score_str.clone(), Style::default().fg(palette.warning)),
                Span::styled(s.title.clone(), Style::default().fg(palette.text)),
            ];

            let used = score_str.len() + s.title.len();

            // Optional extras — try adding comments, tags, user (least important last)
            let extras: Vec<(String, Style)> = vec![
                (
                    format!("  ({}c)", s.comment_count),
                    Style::default().fg(palette.muted),
                ),
                (format!("  {}", tags_str), Style::default().fg(palette.tag)),
                (
                    format!("  by {}", s.submitter_user),
                    Style::default().fg(palette.muted),
                ),
            ];

            let mut remaining = inner_width.saturating_sub(used);
            for (text, style) in &extras {
                if text.len() <= remaining {
                    spans.push(Span::styled(text.clone(), *style));
                    remaining = remaining.saturating_sub(text.len());
                }
            }

            ListItem::new(Line::from(spans))
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

fn draw_comments(f: &mut Frame, app: &mut App, area: Rect, palette: &Palette) {
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
    if display_indices.is_empty() {
        let msg = if app.active_search.is_some() {
            "No matching comments for current search."
        } else {
            "No comments available for this story."
        };
        let empty = Paragraph::new(msg)
            .style(Style::default().fg(palette.muted))
            .block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(empty, area);
        return;
    }
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
                Some(palette.selected_bg)
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
                    .fg(palette.selected_user_fg)
                    .bg(palette.selected_user_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                with_row_bg(
                    Style::default()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD),
                )
            };
            let header = Line::from(vec![
                Span::styled(
                    marker,
                    with_row_bg(
                        Style::default()
                            .fg(palette.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
                ),
                Span::styled(depth_indent.clone(), with_row_bg(Style::default())),
                Span::styled(
                    depth_prefix,
                    with_row_bg(Style::default().fg(palette.muted)),
                ),
                Span::styled(format!(" {} ", c.commenting_user), user_style),
                Span::styled(" ", with_row_bg(Style::default())),
                Span::styled(
                    format!("({})", c.score),
                    with_row_bg(Style::default().fg(palette.warning)),
                ),
            ]);
            let body_style = if selected {
                with_row_bg(
                    Style::default()
                        .fg(palette.text)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Style::default().fg(palette.muted)
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
                    body_style.fg(palette.muted),
                ))]
            } else {
                body_lines
            };

            let mut lines = vec![header];
            lines.extend(body_lines);
            lines.push(Line::from(""));
            if selected {
                for line in lines.iter_mut() {
                    line.style = line.style.bg(palette.selected_bg);
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

fn draw_status(f: &mut Frame, app: &App, area: Rect, palette: &Palette) {
    let mut help = match app.current_view() {
        View::List => {
            "j/k move • enter=comments • o=open link • tab=switch feed • [ ] pgup/pgdn=page • r=refresh • ?=help • q=quit"
        }
        View::Comments => {
            "j/k move • /=search • n=next match • H=high-score • z=collapse • c=comment link • b=open thread • esc=back"
        }
    }
    .to_string();
    let mut hint = String::new();
    if app.is_search_mode() {
        help = format!("SEARCH: {} (Enter apply, Esc cancel)", app.search_input);
        hint = "Recovery: Esc cancels search.".to_string();
    } else if app.is_help_visible() {
        hint = "Recovery: Press ? or Esc to close help.".to_string();
    } else if app.comments_loading {
        hint = "Loading comments… wait or press Esc to return.".to_string();
    } else if app.stories_loading {
        hint = "Loading stories… please wait.".to_string();
    } else if app.status.to_ascii_lowercase().contains("error") {
        hint = "Recovery: press r to retry. Esc returns to list.".to_string();
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
    let state_prefix = if app.status.to_ascii_lowercase().contains("error") {
        "[ERROR]"
    } else if app.comments_loading || app.stories_loading || app.status.starts_with("Loading") {
        "[LOADING]"
    } else {
        "[INFO]"
    };
    let source_token = match app.feed.source() {
        Source::Lobsters => "SRC:L",
        Source::HackerNews => "SRC:H",
    };
    let status_line = format!("{state_prefix} [{source_token}] {status}");
    let help_line = if hint.is_empty() {
        help
    } else {
        format!("{help} | {hint}")
    };
    let text = format!("{status_line}\n{help_line}");
    let p = Paragraph::new(text).style(Style::default().fg(palette.muted));
    f.render_widget(p, area);
}

fn draw_help_overlay(f: &mut Frame, palette: &Palette) {
    f.render_widget(Clear, f.area());

    let area = centered_rect(80, 70, f.area());
    let help = "Help\n\nList: j/k, g/G, Tab, r, [page] / ]page[, PageUp/PageDown, Enter\nComments: j/k, g/G, / search, n next match, H high-score, z collapse, c permalink\nGlobal: o open story, b open thread, p profiling, ? help, q quit, Esc back";
    let panel = Paragraph::new(help).block(
        Block::default()
            .title(" Keybindings ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Rgb(17, 21, 28)).fg(palette.text)),
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
