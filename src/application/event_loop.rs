//! Interactive `crossterm` event loop with input coalescing and buffered frame rendering.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::application::error::AppError;
use crate::application::terminal::TerminalGuard;
use crate::application::workspace::WorkspaceSession;
use crate::config::limits::DEFAULT_RESCAN_MAX_ENTRIES;
use crate::tui::{
    TuiAppState, handle_key_event, handle_mouse_event, render_active_modal, render_tree_view,
};

pub(super) fn run_tui_event_loop(
    session: &mut WorkspaceSession,
    state: &mut TuiAppState,
) -> Result<(), AppError> {
    let mut guard = TerminalGuard::enter()?;
    let mut needs_draw = true;

    while state.exit_action().is_none() {
        if needs_draw {
            guard
                .terminal_mut()
                .draw(|frame| {
                    let area = frame.area();
                    render_tree_view(frame, area, state);
                    render_active_modal(frame, area, state);
                })
                .map_err(|err| AppError::TerminalIo(err.to_string()))?;
            needs_draw = false;
        }

        let first = event::read().map_err(|err| AppError::TerminalIo(err.to_string()))?;
        let mut reingest_needed = dispatch_app_event(state, &first, &mut needs_draw);

        while state.exit_action().is_none()
            && event::poll(Duration::ZERO).map_err(|err| AppError::TerminalIo(err.to_string()))?
        {
            let next = event::read().map_err(|err| AppError::TerminalIo(err.to_string()))?;
            if dispatch_app_event(state, &next, &mut needs_draw) {
                reingest_needed = true;
            }
        }

        if state.exit_action().is_none()
            && reingest_needed
            && session.reingest_pending(state, DEFAULT_RESCAN_MAX_ENTRIES)
        {
            needs_draw = true;
        }
    }
    Ok(())
}

fn dispatch_app_event(state: &mut TuiAppState, ev: &Event, needs_draw: &mut bool) -> bool {
    match ev {
        Event::Key(key) => {
            if key.kind != KeyEventKind::Release {
                handle_key_event(state, *key);
                *needs_draw = true;
            }
        }
        Event::Mouse(mouse) => {
            if handle_mouse_event(state, *mouse) {
                *needs_draw = true;
            }
        }
        Event::Resize(_, _) => {
            *needs_draw = true;
        }
        _ => {}
    }
    state.has_pending_reingest()
}
