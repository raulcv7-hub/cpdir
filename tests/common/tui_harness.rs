//! Shared `ratatui::backend::TestBackend` rendering, input, and cursor helpers for TUI tests.

use cpdir::tui::{
    TuiAppState, TuiViewportAreas, handle_key_event, handle_mouse_event,
    render_active_modal_with_layout, render_tree_view_with_layout,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::style::Color;

/// Syncs viewport scroll and renders tree + modal, recording hit-test boxes in `layout`.
pub(crate) fn draw_screen_with_layout(
    terminal: &mut Terminal<TestBackend>,
    state: &mut TuiAppState,
    layout: &mut TuiViewportAreas,
) {
    terminal
        .draw(|frame| {
            let area = frame.area();
            TuiViewportAreas::sync_scroll(area, state);
            render_tree_view_with_layout(frame, area, state, layout);
            render_active_modal_with_layout(frame, area, state, layout);
        })
        .expect("draw test backend");
}

/// Renders the current UI state into `terminal` and returns the trimmed screen buffer text.
pub(crate) fn draw_and_dump(
    terminal: &mut Terminal<TestBackend>,
    state: &mut TuiAppState,
) -> String {
    let mut layout = TuiViewportAreas::default();
    draw_screen_with_layout(terminal, state, &mut layout);
    buffer_to_string(terminal)
}

/// Converts the terminal cell buffer into trimmed lines joined by newlines.
pub(crate) fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let mut lines = Vec::with_capacity(usize::from(buf.area.height));
    for y in buf.area.y..(buf.area.y + buf.area.height) {
        let mut line = String::new();
        for x in buf.area.x..(buf.area.x + buf.area.width) {
            if let Some(cell) = buf.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    lines.join("\n")
}

/// Returns the foreground color of the first character of `needle` found in the buffer.
pub(crate) fn find_substring_fg(terminal: &Terminal<TestBackend>, needle: &str) -> Option<Color> {
    let buf = terminal.backend().buffer();
    for y in buf.area.y..(buf.area.y + buf.area.height) {
        let mut line = String::new();
        for x in buf.area.x..(buf.area.x + buf.area.width) {
            if let Some(cell) = buf.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        if let Some(byte_idx) = line.find(needle) {
            let char_col = u16::try_from(line[..byte_idx].chars().count()).ok()?;
            return buf.cell((buf.area.x + char_col, y)).map(|c| c.fg);
        }
    }
    None
}

/// Dispatches a synthetic key press (`KeyEventKind::Press`) with no modifiers.
pub(crate) fn press_key(state: &mut TuiAppState, code: KeyCode) {
    handle_key_event(state, KeyEvent::new(code, KeyModifiers::NONE));
}

/// Dispatches a synthetic Win32 `KeyEventKind::Release` event.
pub(crate) fn release_key(state: &mut TuiAppState, code: KeyCode) {
    let mut ev = KeyEvent::new(code, KeyModifiers::NONE);
    ev.kind = KeyEventKind::Release;
    handle_key_event(state, ev);
}

/// Dispatches a left mouse click at `(column, row)` against `layout`.
pub(crate) fn click_left(
    state: &mut TuiAppState,
    layout: &TuiViewportAreas,
    column: u16,
    row: u16,
) -> bool {
    handle_mouse_event(
        state,
        layout,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
    )
}

/// Navigates the tree cursor to the visible row matching `target_rel`.
pub(crate) fn move_cursor_to(state: &mut TuiAppState, target_rel: &str) {
    let Some(target_idx) = state
        .visible_rows()
        .iter()
        .position(|r| state.tree().nodes()[r.node_id.index()].rel_path == target_rel)
    else {
        panic!("missing visible row {target_rel}");
    };
    while state.cursor() != target_idx {
        if state.cursor() < target_idx {
            press_key(state, KeyCode::Down);
        } else {
            press_key(state, KeyCode::Up);
        }
    }
}
