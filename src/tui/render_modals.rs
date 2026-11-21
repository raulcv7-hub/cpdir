//! Modal overlays for the Discarded/Unread category inspector and dialog dispatcher.

use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::domain::metrics::InspectorReason;
use crate::domain::{
    FileDiscardReason, InspectorCategory, InspectorItem, PrunedDirReason, UnselectedReason,
};
use crate::tui::app_state::{TuiAppState, UiMode};
use crate::tui::render_dialogs::{
    render_confirm_empty_modal, render_confirm_secrets_modal, render_help_modal,
};
use crate::tui::theme::{format_token_count, sanitize_tui_label};

/// Renders the active modal overlay on top of the main tree view when `mode` is not [`UiMode::Tree`].
pub fn render_active_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    match state.mode() {
        UiMode::Tree => {}
        UiMode::Inspector { category, scroll } => {
            render_inspector_modal(frame, area, state, category, scroll);
        }
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
    category: InspectorCategory,
    scroll: usize,
) {
    let popup = centered_percent_rect(84, 78, area);
    frame.render_widget(Clear, popup);

    let items: Cow<'_, [InspectorItem]> = match state.active_inspector() {
        Some(ins) if ins.category() == category => Cow::Borrowed(ins.items()),
        _ => Cow::Owned(state.inspector_items(category)),
    };
    let total = items.len();
    let label = match category {
        InspectorCategory::Discarded => "Discarded",
        InspectorCategory::Unread => "Unread",
    };

    let viewport_height = usize::from(popup.height.saturating_sub(2));
    let lines: Vec<Line<'static>> = if items.is_empty() {
        vec![
            Line::raw(""),
            Line::styled(
                format!("  No {label} entries in this workspace."),
                Style::new().fg(Color::Rgb(150, 155, 165)),
            ),
        ]
    } else {
        let start = scroll.min(total.saturating_sub(1));
        let end = start.saturating_add(viewport_height).min(total);
        items[start..end]
            .iter()
            .map(|item| {
                let (tag, tag_style) = reason_badge(item.reason);
                let clean_path = sanitize_tui_label(&item.rel_path);
                let mut spans = vec![
                    Span::raw(" "),
                    Span::styled(format!("[{tag}]"), tag_style),
                    Span::raw(format!(" {clean_path}")),
                ];
                if item.tokens > 0 {
                    spans.push(Span::styled(
                        format!(" ({})", format_token_count(item.tokens)),
                        Style::new()
                            .fg(Color::Rgb(140, 145, 155))
                            .add_modifier(Modifier::DIM),
                    ));
                }
                Line::from(spans)
            })
            .collect()
    };

    let title = format!(" {label} ({total}) - d Discarded | n Unread | Esc Close ");
    let modal = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(Style::new().bg(Color::Rgb(24, 28, 38))),
    );
    frame.render_widget(modal, popup);
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

const fn reason_badge(reason: InspectorReason) -> (&'static str, Style) {
    match reason {
        InspectorReason::PrunedDir(PrunedDirReason::DependencyOrVcs) => {
            ("PRUNED", Style::new().fg(Color::Rgb(150, 155, 165)))
        }
        InspectorReason::DiscardedFile(FileDiscardReason::Binary) => {
            ("BIN", Style::new().fg(Color::Rgb(135, 145, 160)))
        }
        InspectorReason::DiscardedFile(FileDiscardReason::InvalidUtf8) => {
            ("UTF8", Style::new().fg(Color::Rgb(210, 120, 120)))
        }
        InspectorReason::DiscardedFile(FileDiscardReason::ExceedsHardCap { .. }) => {
            (">5MB", Style::new().fg(Color::Rgb(230, 140, 80)))
        }
        InspectorReason::DiscardedFile(FileDiscardReason::Unreadable) => {
            ("ERR", Style::new().fg(Color::Rgb(230, 90, 90)))
        }
        InspectorReason::UnreadFile(UnselectedReason::DotIgnore) => {
            (".ignore", Style::new().fg(Color::Rgb(245, 210, 75)))
        }
        InspectorReason::UnreadFile(UnselectedReason::GitIgnore) => {
            (".gitignore", Style::new().fg(Color::Rgb(185, 165, 90)))
        }
        InspectorReason::UnreadFile(UnselectedReason::ExceedsTokenThreshold) => {
            (">50k", Style::new().fg(Color::Rgb(255, 95, 95)))
        }
        InspectorReason::UnreadFile(UnselectedReason::UserExcluded) => {
            ("off", Style::new().fg(Color::Rgb(150, 155, 165)))
        }
    }
}
