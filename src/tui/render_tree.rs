//! Main tree viewport, live category counters, and dynamic key-legend rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::domain::{
    FileDiscardReason, FolderSelectionState, NodeKind, UnselectedReason, VisibleRow, WeightTier,
};
use crate::tui::app_state::TuiAppState;
use crate::tui::theme::{
    GEMINI_TOKEN_TARGET, NodeVisualKind, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, secret_marker_style, total_tokens_style,
};

/// Renders the header summary, scrollable directory tree, and key-binding legend.
pub fn render_tree_view(frame: &mut Frame<'_>, area: Rect, state: &mut TuiAppState) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .split(area);

    render_header(frame, chunks[0], state);
    render_tree_body(frame, chunks[1], state);
    render_footer(frame, chunks[2], state);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let counts = state.metrics().counts;
    let total_str = format_token_count(counts.selected_tokens);
    let target_str = format_token_count(GEMINI_TOKEN_TARGET);

    let mut spans = vec![
        Span::raw(" Tokens: "),
        Span::styled(
            format!("{total_str} / {target_str}"),
            total_tokens_style(counts.selected_tokens),
        ),
        Span::raw("  |  "),
        Span::styled(
            format!("Read: {}", counts.read_files),
            Style::new().fg(Color::Rgb(85, 220, 125)),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Unread: {}", counts.unread_files),
            Style::new().fg(Color::Rgb(245, 210, 75)),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Discarded: {}", counts.discarded_items),
            Style::new().fg(Color::Rgb(140, 145, 155)),
        ),
    ];

    if counts.active_secret_files > 0 {
        spans.push(Span::raw("  |  "));
        spans.push(Span::styled(
            format!("! Secrets: {}", counts.active_secret_files),
            secret_marker_style(),
        ));
    }

    if state.has_git_context() {
        spans.push(Span::raw("  |  "));
        let (label, style) = if state.include_git_context() {
            ("Git: ON", Style::new().fg(Color::Rgb(85, 220, 125)))
        } else {
            (
                "Git: OFF",
                Style::new()
                    .fg(Color::Rgb(120, 125, 135))
                    .add_modifier(Modifier::DIM),
            )
        };
        spans.push(Span::styled(label, style));
    }

    let root_name = sanitize_tui_label(&state.tree().nodes()[0].name);
    let header = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" cpdir - {root_name} ")),
    );
    frame.render_widget(header, area);
}

fn render_tree_body(frame: &mut Frame<'_>, area: Rect, state: &mut TuiAppState) {
    let viewport_height = usize::from(area.height.saturating_sub(2));
    state.ensure_cursor_visible(viewport_height);

    let scroll = state.scroll_offset();
    let total_rows = state.visible_rows().len();
    let end = scroll.saturating_add(viewport_height).min(total_rows);

    let lines: Vec<Line<'static>> = state.visible_rows()[scroll..end]
        .iter()
        .enumerate()
        .map(|(offset, &row)| build_tree_row_line(state, row, scroll + offset == state.cursor()))
        .collect();

    let title = if total_rows > viewport_height && viewport_height > 0 {
        format!(" Tree ({}..{}/{total_rows}) ", scroll + 1, end)
    } else {
        format!(" Tree ({total_rows}) ")
    };

    let tree_widget =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(tree_widget, area);
}

fn build_tree_row_line(state: &TuiAppState, row: VisibleRow, is_cursor: bool) -> Line<'static> {
    let node = &state.tree().nodes()[row.node_id.index()];
    let rollup = state.metrics().rollups[row.node_id.index()];
    let mut spans = Vec::new();

    spans.push(Span::raw(if is_cursor { "> " } else { "  " }));
    if row.depth > 0 {
        spans.push(Span::raw("  ".repeat(row.depth)));
    }

    let clean_name = sanitize_tui_label(&node.name);
    match &node.kind {
        NodeKind::Directory(dir) => {
            let fold = if dir.is_expanded { "▾ " } else { "▸ " };
            let checkbox = match rollup.folder_selection {
                FolderSelectionState::AllSelected => "[x] ",
                FolderSelectionState::Partial => "[-] ",
                FolderSelectionState::NoneSelected => "[ ] ",
                FolderSelectionState::NoTextFiles => "[·] ",
            };
            let active = rollup.selected_files > 0;
            let tier = rollup.display_weight_tier(active);
            let style = node_label_style(tier, NodeVisualKind::Directory { active });

            let weight_text = format_dir_weight(rollup.selected_tokens, rollup.total_tokens);
            spans.push(Span::styled(
                format!("{fold}{checkbox}{clean_name}/{weight_text}"),
                style,
            ));
            if rollup.has_any_secret {
                spans.push(secret_span(rollup.has_active_secret));
            }
        }
        NodeKind::TextFile(file) => {
            let active = file.state.is_selected();
            let checkbox = if active { "[x] " } else { "[ ] " };
            let tier = WeightTier::from_tokens(file.tokens);
            let style = node_label_style(tier, NodeVisualKind::TextFile { active });
            let tok_str = format_token_count(file.tokens);

            spans.push(Span::styled(
                format!("  {checkbox}{clean_name} ({tok_str})"),
                style,
            ));
            if let Some(reason) = file.state.unselected_reason() {
                spans.push(Span::styled(
                    format!(" [{}]", unselected_tag(reason)),
                    Style::new()
                        .fg(Color::Rgb(110, 115, 125))
                        .add_modifier(Modifier::DIM),
                ));
            }
            if file.secret_hits > 0 {
                spans.push(secret_span(active));
            }
        }
        NodeKind::DiscardedFile(reason) => {
            let tag = discard_tag(*reason);
            let style = node_label_style(WeightTier::Light, NodeVisualKind::Discarded);
            spans.push(Span::styled(format!("  [{tag}] {clean_name}"), style));
        }
    }

    let mut line = Line::from(spans);
    if is_cursor {
        line = line.style(cursor_row_style());
    }
    line
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let git_hint = if state.has_git_context() {
        " | g Git"
    } else {
        ""
    };
    let legend = format!(
        " Enter Copy | Space Toggle | Tab Fold | a All | u None | d Discarded | n Unread{git_hint} | Esc Quit"
    );
    let footer = Paragraph::new(Line::from(Span::styled(
        legend,
        Style::new().fg(Color::Rgb(165, 175, 190)),
    )))
    .block(Block::default().borders(Borders::ALL).title(" Keys "));
    frame.render_widget(footer, area);
}

fn format_dir_weight(selected: usize, total: usize) -> String {
    if total == 0 {
        return String::new();
    }
    if selected == total {
        return format!(" ({})", format_token_count(selected));
    }
    format!(
        " ({}/{})",
        format_token_count(selected),
        format_token_count(total)
    )
}

fn secret_span(active: bool) -> Span<'static> {
    if active {
        Span::styled(" [!]", secret_marker_style())
    } else {
        Span::styled(
            " [!]",
            Style::new()
                .fg(Color::Rgb(150, 75, 95))
                .add_modifier(Modifier::DIM),
        )
    }
}

const fn unselected_tag(reason: UnselectedReason) -> &'static str {
    match reason {
        UnselectedReason::UserExcluded => "off",
        UnselectedReason::DotIgnore => ".ignore",
        UnselectedReason::GitIgnore => ".gitignore",
        UnselectedReason::ExceedsTokenThreshold => ">50k",
    }
}

const fn discard_tag(reason: FileDiscardReason) -> &'static str {
    match reason {
        FileDiscardReason::Binary => "BIN",
        FileDiscardReason::InvalidUtf8 => "UTF8",
        FileDiscardReason::ExceedsHardCap { .. } => ">5MB",
        FileDiscardReason::Unreadable => "ERR",
    }
}
