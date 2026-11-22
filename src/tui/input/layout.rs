//! Single source of truth for viewport geometry, hit-test rectangles, and scroll sync.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders};

use crate::domain::InspectorTab;
use crate::tui::state::TuiAppState;

/// Layout rectangles computed for the inspector modal relative to the screen area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectorSections {
    /// Outer popup rectangle.
    pub popup: Rect,
    /// Two-row tab header + separator rectangle.
    pub tabs: Rect,
    /// Entry list viewport rectangle.
    pub body: Rect,
    /// Bottom status-note row rectangle (`height == 0` when absent).
    pub status: Rect,
}

/// Bounding boxes of interactive viewports stored during render for mouse hit-testing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TuiViewportAreas {
    /// Bounding rectangle of the main directory tree panel (including borders).
    pub tree_body: Rect,
    /// Bounding rectangle of the inspector modal tab header bar.
    pub inspector_tabs: Rect,
    /// Bounding rectangle of the inspector modal entry list viewport.
    pub inspector_body: Rect,
    /// Exact rendered hit-test rectangles for `[Unread, Discarded, Secrets]` tabs.
    pub inspector_tab_rects: [Rect; 3],
    pub(crate) inspector_scroll_top: usize,
}

impl TuiViewportAreas {
    /// Returns the rendered hit-test rectangle for the requested inspector `tab`.
    #[must_use]
    pub const fn inspector_tab_rect(&self, tab: InspectorTab) -> Rect {
        match tab {
            InspectorTab::Unread => self.inspector_tab_rects[0],
            InspectorTab::Discarded => self.inspector_tab_rects[1],
            InspectorTab::Secrets => self.inspector_tab_rects[2],
        }
    }

    /// Synchronizes `state` tree and inspector scroll offsets to `area` before immutable render.
    pub fn sync_scroll(area: Rect, state: &mut TuiAppState) {
        let tree_chunks = tree_screen_chunks(area);
        let tree_h = usize::from(tree_chunks[1].height.saturating_sub(2));
        let has_status = state.inspector().and_then(|i| i.status_note()).is_some();
        let insp_h =
            inspector_sections(area, has_status).map_or(0, |sec| usize::from(sec.body.height));
        state.prepare_viewports(tree_h, insp_h);
    }
}

/// Splits the root terminal `area` into `[header, tree_body, footer]`.
#[must_use]
pub fn tree_screen_chunks(area: Rect) -> [Rect; 3] {
    let header_height = if area.height >= 12 { 4 } else { 3 };
    let chunks = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .split(area);
    [chunks[0], chunks[1], chunks[2]]
}

/// Computes inspector modal section rectangles if `area` is large enough to host the popup.
#[must_use]
pub fn inspector_sections(area: Rect, has_status: bool) -> Option<InspectorSections> {
    let popup = centered_percent_rect(86, 80, area);
    let inner = Block::default().borders(Borders::ALL).inner(popup);
    if inner.height < 3 || inner.width < 10 {
        return None;
    }
    let status_h = u16::from(has_status);
    let sections = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(status_h),
    ])
    .split(inner);

    Some(InspectorSections {
        popup,
        tabs: sections[0],
        body: sections[1],
        status: sections[2],
    })
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
