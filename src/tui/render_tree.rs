//! Responsive tree viewport, live category counters, scrollbar, and adaptive key legend.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::tui::app_state::TuiAppState;
use crate::tui::render_row::build_tree_row_line;
use crate::tui::theme::{
    GEMINI_TOKEN_TARGET, format_token_count, format_token_gauge, sanitize_tui_label,
    secret_marker_style, total_tokens_style,
};

/// Renders the responsive header summary, scrollable tree, and adaptive key-binding legend.
pub fn render_tree_view(frame: &mut Frame<'_>, area: Rect, state: &mut TuiAppState) {
    let header_height = if area.height >= 12 { 4 } else { 3 };
    let chunks = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .split(area);

    render_header(frame, chunks[0], state);
    render_tree_body(frame, chunks[1], state);
    render_footer(frame, chunks[2], state);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let counts = state.metrics().counts;
    let inner_w = usize::from(area.width.saturating_sub(2));
    let total_str = format_token_count(counts.selected_tokens);
    let target_str = format_token_count(GEMINI_TOKEN_TARGET);
    let bar_w = if inner_w >= 74 { 10 } else { 0 };
    let gauge = format_token_gauge(counts.selected_tokens, GEMINI_TOKEN_TARGET, bar_w);

    let mut top_spans = vec![
        Span::raw(" Tokens: "),
        Span::styled(
            format!("{total_str} / {target_str} {gauge}"),
            total_tokens_style(counts.selected_tokens),
        ),
    ];

    if area.height < 4 {
        top_spans.push(Span::raw(" | "));
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
        top_spans.push(git_status_span(state.include_git_context()));
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

fn render_tree_body(frame: &mut Frame<'_>, area: Rect, state: &mut TuiAppState) {
    let viewport_height = usize::from(area.height.saturating_sub(2));
    let inner_width = usize::from(area.width.saturating_sub(2));
    state.ensure_cursor_visible(viewport_height);

    let scroll = state.scroll_offset();
    let total_rows = state.visible_rows().len();
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
        let max_scroll = total_rows.saturating_sub(viewport_height);
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
    let git_hint = if state.has_git_context() {
        " | g Git"
    } else {
        ""
    };
    let legend = if inner_w >= 100 {
        format!(
            " Enter Copy | Space Toggle | Tab Fold | a All | u None | d Discarded | n Unread{git_hint} | Esc Quit"
        )
    } else if inner_w >= 72 {
        format!(" Enter Copy | Space Sel | Tab Fold | a/u | d/n Inspect{git_hint} | Esc Quit")
    } else {
        format!(" Enter Copy | Space Sel | Tab | d/n{git_hint} | Esc")
    };
    let footer = Paragraph::new(Line::from(Span::styled(
        legend,
        Style::new().fg(Color::Rgb(165, 175, 190)),
    )))
    .block(Block::default().borders(Borders::ALL).title(" Keys "));
    frame.render_widget(footer, area);
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

fn git_status_span(enabled: bool) -> Span<'static> {
    if enabled {
        Span::styled("Git: ON", Style::new().fg(Color::Rgb(85, 220, 125)))
    } else {
        Span::styled(
            "Git: OFF",
            Style::new()
                .fg(Color::Rgb(120, 125, 135))
                .add_modifier(Modifier::DIM),
        )
    }
}
