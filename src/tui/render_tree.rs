//! Responsive tree viewport, exact token/git counters, scrollbar, and colorized key legend.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::tui::input::layout::{TuiViewportAreas, tree_screen_chunks};
use crate::tui::render_row::build_tree_row_line;
use crate::tui::state::TuiAppState;
use crate::tui::theme::{
    format_token_count, sanitize_tui_label, secret_marker_style, total_tokens_style,
};

/// Renders the responsive header summary, scrollable tree, and key legend without retaining hit-boxes.
pub fn render_tree_view(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    render_tree_view_with_layout(frame, area, state, &mut TuiViewportAreas::default());
}

/// Renders the responsive header summary, scrollable tree, and key legend, recording `layout`.
pub fn render_tree_view_with_layout(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiAppState,
    layout: &mut TuiViewportAreas,
) {
    let chunks = tree_screen_chunks(area);
    layout.tree_body = chunks[1];

    render_header(frame, chunks[0], state);
    render_tree_body(frame, chunks[1], state);
    render_footer(frame, chunks[2], state);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let counts = state.metrics().counts;
    let inner_w = usize::from(area.width.saturating_sub(2));
    let total_str = format_token_count(state.total_effective_tokens());

    let mut top_spans = vec![
        Span::raw(" Tokens: "),
        Span::styled(total_str, total_tokens_style()),
    ];

    if area.height < 4 {
        top_spans.push(Span::raw("  |  "));
        top_spans.extend(compact_count_spans(counts, inner_w >= 86));
    }

    if counts.active_secret_files > 0 {
        top_spans.push(Span::raw("  |  "));
        top_spans.push(Span::styled(
            format!("! Secrets: {}", counts.active_secret_files),
            secret_marker_style(),
        ));
    }
    if state.has_git_context() {
        top_spans.push(Span::raw("  |  "));
        top_spans.push(git_status_span(
            state.include_git_context(),
            state.git_context_tokens(),
        ));
    }

    let mut lines = vec![Line::from(top_spans)];
    if area.height >= 4 {
        lines.push(Line::from(compact_count_spans(counts, inner_w >= 38)));
    }

    let root_name = sanitize_tui_label(&state.tree().nodes()[0].name);
    let header = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" cpdir - {root_name} ")),
    );
    frame.render_widget(header, area);
}

fn render_tree_body(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let viewport_height = usize::from(area.height.saturating_sub(2));
    let inner_width = usize::from(area.width.saturating_sub(2));
    let total_rows = state.visible_rows().len();
    let max_scroll = total_rows.saturating_sub(viewport_height);
    let scroll = state.scroll_offset().min(max_scroll);
    let end = scroll.saturating_add(viewport_height).min(total_rows);

    let lines: Vec<Line<'static>> = state.visible_rows()[scroll..end]
        .iter()
        .enumerate()
        .map(|(offset, &row)| {
            build_tree_row_line(state, row, scroll + offset == state.cursor(), inner_width)
        })
        .collect();

    let title = if total_rows > viewport_height && viewport_height > 0 {
        format!(" Tree ({}..{}/{total_rows}) ", scroll + 1, end)
    } else {
        format!(" Tree ({total_rows}) ")
    };

    let tree_widget =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(tree_widget, area);

    if total_rows > viewport_height && viewport_height > 0 {
        let mut sb_state = ScrollbarState::new(max_scroll + 1).position(scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::new().fg(Color::Rgb(110, 210, 255)));
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let inner_w = usize::from(area.width.saturating_sub(2));
    let mut spans = vec![Span::raw(" ")];
    let wide = inner_w >= 100;
    let medium = inner_w >= 72;

    push_key_hint(
        &mut spans,
        "[Enter]",
        if medium { " Copy" } else { "" },
        Color::Rgb(85, 220, 125),
    );
    spans.push(Span::raw(if wide { "  " } else { " " }));
    push_key_hint(
        &mut spans,
        "[Space]",
        if wide {
            " Toggle"
        } else if medium {
            "Sel"
        } else {
            ""
        },
        Color::Rgb(110, 210, 255),
    );
    spans.push(Span::raw(if wide { "  " } else { " " }));
    push_key_hint(
        &mut spans,
        "[Tab]",
        if medium { " Fold" } else { "" },
        Color::Rgb(245, 210, 75),
    );
    spans.push(Span::raw(if wide { "  " } else { " " }));
    push_key_hint(
        &mut spans,
        "[a]",
        if wide {
            " All/None"
        } else if medium {
            "All"
        } else {
            ""
        },
        Color::Rgb(245, 210, 75),
    );
    spans.push(Span::raw(if wide { "  " } else { " " }));
    push_key_hint(
        &mut spans,
        "[i]",
        if wide {
            " Inspect"
        } else if medium {
            " Insp"
        } else {
            ""
        },
        Color::Rgb(140, 185, 255),
    );
    if state.has_git_context() {
        spans.push(Span::raw(if wide { "  " } else { " " }));
        let git_color = if state.include_git_context() {
            Color::Rgb(85, 220, 125)
        } else {
            Color::Rgb(130, 135, 145)
        };
        push_key_hint(&mut spans, "[g]", " Git", git_color);
    }
    spans.push(Span::raw(if wide { "  " } else { " " }));
    push_key_hint(
        &mut spans,
        "[?]",
        if wide { " Help" } else { "" },
        Color::Rgb(205, 130, 255),
    );
    spans.push(Span::raw(if wide { "  " } else { " " }));
    push_key_hint(
        &mut spans,
        "[Esc]",
        if medium { " Quit" } else { "" },
        Color::Rgb(255, 115, 115),
    );

    let footer = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(" Keys "));
    frame.render_widget(footer, area);
}

fn push_key_hint(
    spans: &mut Vec<Span<'static>>,
    key: &'static str,
    label: &'static str,
    fg: Color,
) {
    spans.push(Span::styled(
        key,
        Style::new().fg(fg).add_modifier(Modifier::BOLD),
    ));
    if !label.is_empty() {
        spans.push(Span::styled(
            label,
            Style::new().fg(Color::Rgb(195, 202, 214)),
        ));
    }
}

fn compact_count_spans(counts: crate::domain::CategoryCounts, verbose: bool) -> Vec<Span<'static>> {
    let (r_lbl, u_lbl, d_lbl) = if verbose {
        (" Read:", "  Unread:", "  Discarded:")
    } else {
        (" R:", "  U:", "  D:")
    };
    vec![
        Span::styled(
            format!("{r_lbl} {}", counts.read_files),
            Style::new().fg(Color::Rgb(85, 220, 125)),
        ),
        Span::styled(
            format!("{u_lbl} {}", counts.unread_files),
            Style::new().fg(Color::Rgb(245, 210, 75)),
        ),
        Span::styled(
            format!("{d_lbl} {}", counts.discarded_items),
            Style::new().fg(Color::Rgb(140, 145, 155)),
        ),
    ]
}

fn git_status_span(enabled: bool, git_tokens: usize) -> Span<'static> {
    if enabled {
        let label = if git_tokens > 0 {
            format!("Git: ON (+{})", format_token_count(git_tokens))
        } else {
            "Git: ON".to_owned()
        };
        Span::styled(label, Style::new().fg(Color::Rgb(85, 220, 125)))
    } else {
        Span::styled(
            "Git: OFF",
            Style::new()
                .fg(Color::Rgb(120, 125, 135))
                .add_modifier(Modifier::DIM),
        )
    }
}
