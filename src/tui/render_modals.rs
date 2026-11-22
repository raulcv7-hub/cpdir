//! Modal overlays for the unified inspector and confirmation/help dialogs.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::domain::InspectorTab;
use crate::tui::app_state::{TuiAppState, UiMode};
use crate::tui::render_dialogs::{
    render_confirm_empty_modal, render_confirm_secrets_modal, render_help_modal,
};
use crate::tui::render_inspector_row::build_inspector_entry_line;

/// Renders the active modal overlay on top of the main tree view when `mode` is not [`UiMode::Tree`].
pub fn render_active_modal(frame: &mut Frame<'_>, area: Rect, state: &mut TuiAppState) {
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
    state: &mut TuiAppState,
    tab: InspectorTab,
) {
    let popup = centered_percent_rect(86, 80, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Inspector - [Tab] Switch  [Space] Toggle/Load  [Enter] Jump  [Esc] Close ")
        .style(Style::new().bg(Color::Rgb(24, 28, 38)));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let status_h = u16::from(
        state
            .active_inspector()
            .and_then(|i| i.status_note())
            .is_some(),
    );
    let sections = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(status_h),
    ])
    .split(inner);

    let viewport_h = usize::from(sections[1].height);
    state.ensure_inspector_cursor_visible(viewport_h);

    let counts = state.metrics().counts;
    let tab_rects = render_tab_header(
        frame,
        sections[0],
        tab,
        counts.unread_files,
        counts.discarded_items,
        counts.total_secret_files,
    );

    state.viewport_areas.inspector_tabs = sections[0];
    state.viewport_areas.inspector_tab_rects = tab_rects;
    state.viewport_areas.inspector_body = sections[1];

    let (total, start, lines, status_note) = {
        let Some(ins) = state.active_inspector() else {
            return;
        };
        let entries = ins.entries();
        let total = entries.len();
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
        (total, start, lines, ins.status_note().map(str::to_owned))
    };

    state.viewport_areas.inspector_scroll_top = start;
    frame.render_widget(Paragraph::new(lines), sections[1]);

    if let Some(note) = status_note {
        let status_line = Line::styled(
            format!(" {note}"),
            Style::new()
                .fg(Color::Rgb(110, 210, 255))
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(Paragraph::new(status_line), sections[2]);
    }

    if total > viewport_h && viewport_h > 0 && popup.height >= 7 {
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
) -> [Rect; 3] {
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

    let max_x = area.x.saturating_add(area.width);
    let mut x = area.x.saturating_add(1);
    let mut tab_rects = [Rect::default(); 3];
    let mut spans = vec![Span::raw(" ")];

    for (idx, (tab, label, color)) in tabs.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" | ", Style::new().fg(Color::Rgb(75, 82, 98))));
            x = x.saturating_add(3);
        }
        let label_w = u16::try_from(Span::raw(label.as_str()).width()).unwrap_or(u16::MAX);
        let clipped_w = label_w.min(max_x.saturating_sub(x));
        tab_rects[idx] = Rect::new(x, area.y, clipped_w, 1);
        x = x.saturating_add(label_w);

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
    tab_rects
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
