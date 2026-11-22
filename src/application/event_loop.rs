//! Interactive `crossterm` event loop with input coalescing and buffered frame rendering.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::application::error::AppError;
use crate::application::terminal::TerminalGuard;
use crate::application::workspace::WorkspaceSession;
use crate::config::limits::DEFAULT_RESCAN_MAX_ENTRIES;
use crate::tui::{
    TuiAppState, TuiViewportAreas, handle_key_event, handle_mouse_event,
    render_active_modal_with_layout, render_tree_view_with_layout,
};

pub(super) fn run_tui_event_loop(
    session: &mut WorkspaceSession,
    state: &mut TuiAppState,
) -> Result<(), AppError> {
    let mut guard = TerminalGuard::enter()?;
    let mut layout = TuiViewportAreas::default();
    let mut needs_draw = true;
    let mut last_fp = state.overhead_fingerprint();

    while state.exit_action().is_none() {
        if needs_draw {
            guard
                .terminal_mut()
                .draw(|frame| {
                    let area = frame.area();
                    TuiViewportAreas::sync_scroll(area, state);
                    render_tree_view_with_layout(frame, area, state, &mut layout);
                    render_active_modal_with_layout(frame, area, state, &mut layout);
                })
                .map_err(|err| AppError::TerminalIo(err.to_string()))?;
            needs_draw = false;
        }

        let first = event::read().map_err(|err| AppError::TerminalIo(err.to_string()))?;
        let mut reingest_needed = dispatch_app_event(state, &layout, &first, &mut needs_draw);

        while state.exit_action().is_none()
            && event::poll(Duration::ZERO).map_err(|err| AppError::TerminalIo(err.to_string()))?
        {
            let next = event::read().map_err(|err| AppError::TerminalIo(err.to_string()))?;
            if dispatch_app_event(state, &layout, &next, &mut needs_draw) {
                reingest_needed = true;
            }
        }

        if state.exit_action().is_none()
            && reingest_needed
            && session.reingest_pending(state, DEFAULT_RESCAN_MAX_ENTRIES)
        {
            last_fp = state.overhead_fingerprint();
            needs_draw = true;
        }

        if state.exit_action().is_none() {
            let next_fp = state.overhead_fingerprint();
            if next_fp != last_fp {
                session.refresh_xml_overhead(state);
                last_fp = next_fp;
            }
        }
    }
    Ok(())
}

fn dispatch_app_event(
    state: &mut TuiAppState,
    layout: &TuiViewportAreas,
    ev: &Event,
    needs_draw: &mut bool,
) -> bool {
    match ev {
        Event::Key(key) => {
            if key.kind != KeyEventKind::Release {
                handle_key_event(state, *key);
                *needs_draw = true;
            }
        }
        Event::Mouse(mouse) => {
            if handle_mouse_event(state, layout, *mouse) {
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
