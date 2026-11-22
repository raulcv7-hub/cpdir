//! Main tree viewport renderer uniting adaptive chrome, scrollable list, and scrollbar.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::tui::input::layout::{TuiViewportAreas, tree_screen_chunks};
use crate::tui::state::TuiAppState;
use crate::tui::view::tree_chrome::{render_footer, render_header};
use crate::tui::view::tree_row::build_tree_row_line;

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
