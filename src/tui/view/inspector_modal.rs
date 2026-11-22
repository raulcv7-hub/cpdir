//! Main frame, list body, status line, and scrollbar rendering for the unified inspector modal.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::domain::InspectorTab;
use crate::tui::input::layout::{TuiViewportAreas, inspector_sections};
use crate::tui::state::TuiAppState;
use crate::tui::view::inspector_row::build_inspector_entry_line;
use crate::tui::view::inspector_tabs::{compute_visible_window, render_tab_header, tab_name};

pub(super) fn render_inspector_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiAppState,
    layout: &mut TuiViewportAreas,
    tab: InspectorTab,
) {
    let Some(ins) = state.inspector() else {
        return;
    };
    let has_status = ins.status_note().is_some();
    let Some(sections) = inspector_sections(area, has_status) else {
        return;
    };

    frame.render_widget(Clear, sections.popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Inspector - [Tab] Switch  [Space] Toggle/Load  [Enter] Jump  [Esc] Close ")
        .style(Style::new().bg(Color::Rgb(24, 28, 38)));
    frame.render_widget(block, sections.popup);

    let counts = state.metrics().counts;
    let tab_rects = render_tab_header(
        frame,
        sections.tabs,
        tab,
        counts.unread_files,
        counts.discarded_items,
        counts.total_secret_files,
    );

    layout.inspector_tabs = sections.tabs;
    layout.inspector_tab_rects = tab_rects;
    layout.inspector_body = sections.body;

    let entries = ins.entries();
    let total = entries.len();
    let viewport_h = usize::from(sections.body.height);
    let (start, end) = compute_visible_window(ins.cursor(), ins.scroll(), total, viewport_h);
    layout.inspector_scroll_top = start;

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

    frame.render_widget(Paragraph::new(lines), sections.body);

    if let Some(note) = ins.status_note() {
        let status_line = Line::styled(
            format!(" {note}"),
            Style::new()
                .fg(Color::Rgb(110, 210, 255))
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(Paragraph::new(status_line), sections.status);
    }

    if total > viewport_h && viewport_h > 0 && sections.popup.height >= 7 {
        let max_scroll = total.saturating_sub(viewport_h);
        let mut sb_state = ScrollbarState::new(max_scroll + 1).position(start);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::new().fg(Color::Rgb(110, 210, 255)));
        frame.render_stateful_widget(
            scrollbar,
            sections.popup.inner(Margin {
                vertical: 3,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
}
