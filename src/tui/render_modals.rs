//! Unified tabbed inspector overlay for Unread, Discarded, and Secrets entries.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::domain::{FileDiscardReason, NodeKind, PrunedDirReason, UnselectedReason};
use crate::tui::app_state::{TuiAppState, UiMode};
use crate::tui::inspector_state::{InspectorEntry, InspectorEntryKind, InspectorTab};
use crate::tui::render_dialogs::{
    render_confirm_empty_modal, render_confirm_secrets_modal, render_help_modal,
};
use crate::tui::theme::{
    cursor_glyph_style, cursor_row_style, format_token_count, sanitize_tui_label,
    secret_marker_style, secret_rule_label,
};

/// Renders the active modal overlay on top of the main tree view when `mode` is not [`UiMode::Tree`].
pub fn render_active_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    match state.mode() {
        UiMode::Tree => {}
        UiMode::Inspector { tab, .. } => render_inspector_modal(frame, area, state, tab),
        UiMode::Help => render_help_modal(frame, area, state),
        UiMode::ConfirmEmptyCopy => render_confirm_empty_modal(frame, area),
        UiMode::ConfirmSecrets {
            active_secret_files,
        } => {
            render_confirm_secrets_modal(frame, area, state, active_secret_files);
        }
    }
}

fn render_inspector_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiAppState,
    tab: InspectorTab,
) {
    let Some(ins) = state.active_inspector() else {
        return;
    };
    let popup = centered_percent_rect(86, 80, area);
    frame.render_widget(Clear, popup);

    let entries = ins.entries();
    let total = entries.len();
    let counts = state.metrics().counts;

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Inspector - [Tab] Switch  [Space] Toggle  [Enter] Jump  [Esc] Close ")
        .style(Style::new().bg(Color::Rgb(24, 28, 38)));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let sections = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(inner);
    render_tab_header(
        frame,
        sections[0],
        tab,
        counts.unread_files,
        counts.discarded_items,
        counts.total_secret_files,
    );

    let viewport_h = usize::from(sections[1].height);
    let (start, end) = compute_visible_window(ins.cursor(), ins.scroll(), total, viewport_h);

    let lines: Vec<Line<'static>> = if entries.is_empty() {
        vec![
            Line::raw(""),
            Line::styled(
                format!("  No {} entries in this workspace.", tab_name(tab)),
                Style::new().fg(Color::Rgb(150, 155, 165)),
            ),
        ]
    } else {
        entries[start..end]
            .iter()
            .enumerate()
            .map(|(offset, entry)| {
                build_inspector_entry_line(state, entry, start + offset == ins.cursor())
            })
            .collect()
    };

    frame.render_widget(Paragraph::new(lines), sections[1]);

    if total > viewport_h && viewport_h > 0 {
        let max_scroll = total.saturating_sub(viewport_h);
        let mut sb_state = ScrollbarState::new(max_scroll + 1).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::new().fg(Color::Rgb(110, 210, 255)));
        frame.render_stateful_widget(
            scrollbar,
            popup.inner(Margin {
                vertical: 3,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
}

fn render_tab_header(
    frame: &mut Frame<'_>,
    area: Rect,
    active: InspectorTab,
    unread: usize,
    discarded: usize,
    secrets: usize,
) {
    let tabs = [
        (
            InspectorTab::Unread,
            format!(" Unread ({unread}) "),
            Color::Rgb(245, 210, 75),
        ),
        (
            InspectorTab::Discarded,
            format!(" Discarded ({discarded}) "),
            Color::Rgb(150, 165, 185),
        ),
        (
            InspectorTab::Secrets,
            format!(" Secrets ({secrets}) "),
            Color::Rgb(205, 130, 255),
        ),
    ];

    let mut spans = vec![Span::raw(" ")];
    for (idx, (tab, label, color)) in tabs.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" | ", Style::new().fg(Color::Rgb(75, 82, 98))));
        }
        let style = if tab == active {
            Style::new()
                .fg(Color::Rgb(20, 24, 32))
                .bg(color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(color)
        };
        spans.push(Span::styled(label, style));
    }

    let sep = "─".repeat(usize::from(area.width));
    let widget = Paragraph::new(vec![
        Line::from(spans),
        Line::styled(sep, Style::new().fg(Color::Rgb(75, 82, 98))),
    ]);
    frame.render_widget(widget, area);
}

fn build_inspector_entry_line(
    state: &TuiAppState,
    entry: &InspectorEntry,
    is_cursor: bool,
) -> Line<'static> {
    let mut spans = Vec::new();
    if is_cursor {
        spans.push(Span::styled("▶ ", cursor_glyph_style()));
    } else {
        spans.push(Span::raw("  "));
    }

    let clean_path = sanitize_tui_label(&entry.rel_path);
    match entry.kind {
        InspectorEntryKind::UnreadDir {
            node_id,
            unread_count,
            tokens,
        } => {
            let active = state.metrics().rollups[node_id.index()].selected_files > 0;
            spans.push(checkbox_span(active));
            spans.push(Span::styled(
                "[DIR]",
                Style::new()
                    .fg(Color::Rgb(110, 210, 255))
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(format!(
                " {clean_path}/ ({unread_count} files, {})",
                format_token_count(tokens)
            )));
        }
        InspectorEntryKind::UnreadFile {
            node_id,
            reason,
            tokens,
        } => {
            let active = is_node_selected(state, node_id);
            let (tag, tag_style) = unselected_badge(reason);
            spans.push(checkbox_span(active));
            spans.push(Span::styled(format!("[{tag}]"), tag_style));
            spans.push(Span::raw(format!(" {clean_path}")));
            if tokens > 0 {
                spans.push(Span::styled(
                    format!(" ({})", format_token_count(tokens)),
                    Style::new()
                        .fg(Color::Rgb(140, 145, 155))
                        .add_modifier(Modifier::DIM),
                ));
            }
        }
        InspectorEntryKind::DiscardedPrunedDir {
            reason: PrunedDirReason::DependencyOrVcs,
        } => {
            spans.push(Span::raw("    "));
            spans.push(Span::styled(
                "[PRUNED]",
                Style::new().fg(Color::Rgb(150, 155, 165)),
            ));
            spans.push(Span::raw(format!(" {clean_path}/")));
        }
        InspectorEntryKind::DiscardedFile { reason, .. } => {
            let (tag, tag_style) = discard_badge(reason);
            spans.push(Span::raw("    "));
            spans.push(Span::styled(format!("[{tag}]"), tag_style));
            spans.push(Span::raw(format!(" {clean_path}")));
        }
        InspectorEntryKind::SecretItem {
            node_id,
            line,
            rule,
            hits,
        } => {
            let active = is_node_selected(state, node_id);
            spans.push(checkbox_span(active));
            let badge = rule.map_or("SECRET", secret_rule_label);
            spans.push(Span::styled(format!("[{badge}]"), secret_marker_style()));
            let loc = match line {
                Some(ln) => format!(" {clean_path}:{ln}"),
                None => format!(" {clean_path} ({hits} hit(s))"),
            };
            spans.push(Span::styled(
                loc,
                Style::new().fg(if active {
                    Color::Rgb(205, 130, 255)
                } else {
                    Color::Rgb(140, 110, 165)
                }),
            ));
        }
    }

    let mut row = Line::from(spans);
    if is_cursor {
        row = row.style(cursor_row_style());
    }
    row
}

fn is_node_selected(state: &TuiAppState, id: crate::domain::NodeId) -> bool {
    matches!(
        state.tree().get(id).map(|n| &n.kind),
        Some(NodeKind::TextFile(f)) if f.state.is_selected()
    )
}

fn checkbox_span(active: bool) -> Span<'static> {
    if active {
        Span::styled(
            "[x] ",
            Style::new()
                .fg(Color::Rgb(85, 220, 125))
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("[ ] ", Style::new().fg(Color::Rgb(130, 135, 145)))
    }
}

fn compute_visible_window(
    cursor: usize,
    scroll: usize,
    total: usize,
    height: usize,
) -> (usize, usize) {
    if total == 0 || height == 0 {
        return (0, 0);
    }
    let max_start = total.saturating_sub(height);
    let mut start = scroll.min(max_start);
    if cursor < start {
        start = cursor;
    } else if cursor >= start + height {
        start = cursor + 1 - height;
    }
    (start, start.saturating_add(height).min(total))
}

fn centered_percent_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let margin_y = 100u16.saturating_sub(percent_y) / 2;
    let margin_x = 100u16.saturating_sub(percent_x) / 2;

    let vertical = Layout::vertical([
        Constraint::Percentage(margin_y),
        Constraint::Percentage(percent_y),
        Constraint::Percentage(margin_y),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage(margin_x),
        Constraint::Percentage(percent_x),
        Constraint::Percentage(margin_x),
    ])
    .split(vertical[1])[1]
}

const fn tab_name(tab: InspectorTab) -> &'static str {
    match tab {
        InspectorTab::Unread => "Unread",
        InspectorTab::Discarded => "Discarded",
        InspectorTab::Secrets => "Secrets",
    }
}

const fn unselected_badge(reason: UnselectedReason) -> (&'static str, Style) {
    match reason {
        UnselectedReason::DotIgnore => (".ignore", Style::new().fg(Color::Rgb(245, 210, 75))),
        UnselectedReason::GitIgnore => (".gitignore", Style::new().fg(Color::Rgb(185, 165, 90))),
        UnselectedReason::ExceedsTokenThreshold => {
            (">50k", Style::new().fg(Color::Rgb(255, 95, 95)))
        }
        UnselectedReason::UserExcluded => ("off", Style::new().fg(Color::Rgb(150, 155, 165))),
    }
}

const fn discard_badge(reason: FileDiscardReason) -> (&'static str, Style) {
    match reason {
        FileDiscardReason::Binary => ("BIN", Style::new().fg(Color::Rgb(135, 145, 160))),
        FileDiscardReason::InvalidUtf8 => ("UTF8", Style::new().fg(Color::Rgb(210, 120, 120))),
        FileDiscardReason::ExceedsHardCap { .. } => {
            (">5MB", Style::new().fg(Color::Rgb(230, 140, 80)))
        }
        FileDiscardReason::Unreadable => ("ERR", Style::new().fg(Color::Rgb(230, 90, 90))),
    }
}
