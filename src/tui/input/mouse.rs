//! Mouse event dispatcher for wheel scrolling, row selection, and tab clicks.

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::domain::{InspectorTab, NodeKind};
use crate::tui::input::layout::TuiViewportAreas;
use crate::tui::state::{TuiAppState, UiMode};

/// Applies a mouse event to [`TuiAppState`] using `layout`, returning `true` when redraw is needed.
#[must_use]
pub fn handle_mouse_event(
    state: &mut TuiAppState,
    layout: &TuiViewportAreas,
    mouse: MouseEvent,
) -> bool {
    match mouse.kind {
        MouseEventKind::ScrollDown => handle_mouse_scroll(state, 2),
        MouseEventKind::ScrollUp => handle_mouse_scroll(state, -2),
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_left_click(state, layout, mouse.column, mouse.row)
        }
        _ => false,
    }
}

fn handle_mouse_scroll(state: &mut TuiAppState, delta: isize) -> bool {
    match state.mode() {
        UiMode::Tree => {
            if delta.is_negative() {
                state.nav_mut().page_up(delta.unsigned_abs());
            } else {
                state.nav_mut().page_down(delta.cast_unsigned());
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

fn handle_mouse_left_click(
    state: &mut TuiAppState,
    layout: &TuiViewportAreas,
    col: u16,
    row: u16,
) -> bool {
    match state.mode() {
        UiMode::Tree => handle_tree_click(state, layout, col, row),
        UiMode::Inspector { .. } => handle_inspector_click(state, layout, col, row),
        UiMode::Help => {
            state.close_modal();
            true
        }
        UiMode::ConfirmEmptyCopy | UiMode::ConfirmSecrets { .. } => false,
    }
}

fn handle_tree_click(
    state: &mut TuiAppState,
    layout: &TuiViewportAreas,
    col: u16,
    row: u16,
) -> bool {
    let area = layout.tree_body;
    let min_x = area.x.saturating_add(1);
    let max_x = area.x.saturating_add(area.width.saturating_sub(1));
    let min_y = area.y.saturating_add(1);
    let max_y = area.y.saturating_add(area.height.saturating_sub(1));

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

    state.nav_mut().set_cursor(target_idx);
    if is_dir && col_offset >= fold_glyph_start && col_offset < fold_glyph_start + 2 {
        state.toggle_expand_at_index(target_idx);
        return true;
    }

    state.toggle_selection_at_cursor();
    true
}

fn handle_inspector_click(
    state: &mut TuiAppState,
    layout: &TuiViewportAreas,
    col: u16,
    row: u16,
) -> bool {
    let tabs = [
        InspectorTab::Unread,
        InspectorTab::Discarded,
        InspectorTab::Secrets,
    ];
    for (rect, tab) in layout.inspector_tab_rects.into_iter().zip(tabs) {
        if rect.width > 0
            && row == rect.y
            && col >= rect.x
            && col < rect.x.saturating_add(rect.width)
        {
            state.open_inspector_tab(tab);
            return true;
        }
    }

    let body_area = layout.inspector_body;
    if col >= body_area.x
        && col < body_area.x.saturating_add(body_area.width)
        && row >= body_area.y
        && row < body_area.y.saturating_add(body_area.height)
    {
        let offset = usize::from(row - body_area.y);
        let Some(ins) = state.inspector() else {
            return false;
        };
        let target_idx = layout.inspector_scroll_top.saturating_add(offset);
        if target_idx < ins.entries().len() {
            state.set_inspector_cursor(target_idx);
            state.toggle_inspector_selection();
            return true;
        }
    }
    false
}
