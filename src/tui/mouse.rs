//! Mouse event dispatcher for wheel scrolling, row selection, and tab clicks.

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::domain::NodeKind;
use crate::tui::app_state::{TuiAppState, UiMode};
use crate::tui::inspector_items::InspectorTab;

/// Bounding boxes of interactive viewports stored during render for mouse hit-testing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TuiViewportAreas {
    pub(crate) tree_body: Rect,
    pub(crate) inspector_tabs: Rect,
    pub(crate) inspector_body: Rect,
}

/// Applies a mouse event to [`TuiAppState`], returning `true` if state changed and needs redraw.
#[must_use]
pub fn handle_mouse_event(state: &mut TuiAppState, mouse: MouseEvent) -> bool {
    match mouse.kind {
        MouseEventKind::ScrollDown => handle_mouse_scroll(state, 2),
        MouseEventKind::ScrollUp => handle_mouse_scroll(state, -2),
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_left_click(state, mouse.column, mouse.row)
        }
        _ => false,
    }
}

fn handle_mouse_scroll(state: &mut TuiAppState, delta: isize) -> bool {
    match state.mode() {
        UiMode::Tree => {
            if delta.is_negative() {
                state.page_up(delta.unsigned_abs());
            } else {
                state.page_down(delta.cast_unsigned());
            }
            true
        }
        UiMode::Inspector { .. } => {
            state.scroll_inspector(delta);
            true
        }
        UiMode::Help | UiMode::ConfirmEmptyCopy | UiMode::ConfirmSecrets { .. } => false,
    }
}

fn handle_mouse_left_click(state: &mut TuiAppState, col: u16, row: u16) -> bool {
    match state.mode() {
        UiMode::Tree => handle_tree_click(state, col, row),
        UiMode::Inspector { .. } => handle_inspector_click(state, col, row),
        UiMode::Help => {
            state.close_modal();
            true
        }
        UiMode::ConfirmEmptyCopy | UiMode::ConfirmSecrets { .. } => false,
    }
}

fn handle_tree_click(state: &mut TuiAppState, col: u16, row: u16) -> bool {
    let area = state.viewport_areas.tree_body;
    let min_x = area.x.saturating_add(1);
    let max_x = area.x + area.width.saturating_sub(1);
    let min_y = area.y.saturating_add(1);
    let max_y = area.y + area.height.saturating_sub(1);

    if col < min_x || col >= max_x || row < min_y || row >= max_y {
        return false;
    }

    let row_offset = usize::from(row - min_y);
    let target_idx = state.scroll_offset() + row_offset;
    let Some(row_meta) = state.visible_rows().get(target_idx).copied() else {
        return false;
    };

    let col_offset = usize::from(col - min_x);
    let fold_glyph_start = 2 + row_meta.depth * 2;
    let is_dir = matches!(
        state.tree().nodes()[row_meta.node_id.index()].kind,
        NodeKind::Directory(_)
    );

    if is_dir && col_offset >= fold_glyph_start && col_offset < fold_glyph_start + 2 {
        state.toggle_expand_at_index(target_idx);
        return true;
    }

    state.cursor = target_idx;
    state.toggle_selection_at_cursor();
    true
}

fn handle_inspector_click(state: &mut TuiAppState, col: u16, row: u16) -> bool {
    let tabs_area = state.viewport_areas.inspector_tabs;
    if row == tabs_area.y && col >= tabs_area.x && col < tabs_area.x + tabs_area.width {
        let rel_x = col - tabs_area.x;
        let tab = if rel_x < 15 {
            InspectorTab::Unread
        } else if rel_x < 33 {
            InspectorTab::Discarded
        } else {
            InspectorTab::Secrets
        };
        state.open_inspector_tab(tab);
        return true;
    }

    let body_area = state.viewport_areas.inspector_body;
    if col >= body_area.x
        && col < body_area.x + body_area.width
        && row >= body_area.y
        && row < body_area.y + body_area.height
    {
        let offset = usize::from(row - body_area.y);
        let Some(ins) = state.active_inspector() else {
            return false;
        };
        let target_idx = ins.scroll() + offset;
        if target_idx < ins.entries().len() {
            state.set_inspector_cursor(target_idx);
            state.toggle_inspector_selection();
            return true;
        }
    }
    false
}
