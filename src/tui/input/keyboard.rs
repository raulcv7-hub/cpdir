//! Pure key-event dispatcher translating `crossterm` inputs into [`TuiAppState`] transitions.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::domain::InspectorTab;
use crate::tui::state::{TuiAppState, UiMode};

/// Applies a single keyboard event to [`TuiAppState`], ignoring Win32 `Release` events.
pub fn handle_key_event(state: &mut TuiAppState, key: KeyEvent) {
    // Windows console emits Press + Release per keystroke; ignoring Release avoids double toggles.
    if key.kind == KeyEventKind::Release {
        return;
    }

    if matches!(key.code, KeyCode::Char('c' | 'C')) && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        state.cancel_exit();
        return;
    }

    match state.mode() {
        UiMode::Tree => handle_tree_key(state, key.code),
        UiMode::Inspector { .. } => handle_inspector_key(state, key.code),
        UiMode::Help => handle_help_key(state, key.code),
        UiMode::ConfirmEmptyCopy => handle_confirm_key(state, key.code, false),
        UiMode::ConfirmSecrets { .. } => handle_confirm_key(state, key.code, true),
    }
}

fn handle_tree_key(state: &mut TuiAppState, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k' | 'K') => state.nav_mut().move_up(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => state.nav_mut().move_down(),
        KeyCode::Home => state.nav_mut().move_home(),
        KeyCode::End => state.nav_mut().move_end(),
        KeyCode::PageUp => state.nav_mut().page_up(15),
        KeyCode::PageDown => state.nav_mut().page_down(15),
        KeyCode::Right | KeyCode::Char('l' | 'L') => state.expand_or_enter_at_cursor(),
        KeyCode::Left | KeyCode::Char('h' | 'H') => state.collapse_or_jump_parent(),
        KeyCode::Tab => state.toggle_expand_at_cursor(),
        KeyCode::Char('c' | 'C') => state.collapse_all(),
        KeyCode::Char('e' | 'E') => state.expand_all(),
        KeyCode::Char(' ') => state.toggle_selection_at_cursor(),
        KeyCode::Char('a' | 'A') => state.toggle_all_selection(),
        KeyCode::Char('u' | 'U') => state.unselect_all(),
        KeyCode::Char('g' | 'G') => state.toggle_git_context(),
        KeyCode::Char('p' | 'P') => state.toggle_abs_path(),
        KeyCode::Char('i' | 'I' | 'n' | 'N') => state.open_inspector_tab(InspectorTab::Unread),
        KeyCode::Char('d' | 'D') => state.open_inspector_tab(InspectorTab::Discarded),
        KeyCode::Char('s' | 'S') => state.open_inspector_tab(InspectorTab::Secrets),
        KeyCode::Char('?') => state.toggle_help(),
        KeyCode::Enter => state.request_copy(),
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => state.cancel_exit(),
        _ => {}
    }
}

fn handle_inspector_key(state: &mut TuiAppState, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k' | 'K') => state.scroll_inspector(-1),
        KeyCode::Down | KeyCode::Char('j' | 'J') => state.scroll_inspector(1),
        KeyCode::Home => state.inspector_home(),
        KeyCode::End => state.inspector_end(),
        KeyCode::PageUp => state.scroll_inspector(-15),
        KeyCode::PageDown => state.scroll_inspector(15),
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l' | 'L') => {
            state.cycle_inspector_tab(true);
        }
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h' | 'H') => {
            state.cycle_inspector_tab(false);
        }
        KeyCode::Char('n' | 'N') => state.open_inspector_tab(InspectorTab::Unread),
        KeyCode::Char('d' | 'D') => state.open_inspector_tab(InspectorTab::Discarded),
        KeyCode::Char('s' | 'S') => state.open_inspector_tab(InspectorTab::Secrets),
        KeyCode::Char(' ') => state.toggle_inspector_selection(),
        KeyCode::Enter => state.jump_to_inspector_entry(),
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => state.close_modal(),
        _ => {}
    }
}

fn handle_help_key(state: &mut TuiAppState, code: KeyCode) {
    if matches!(
        code,
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?' | 'q' | 'Q')
    ) {
        state.close_modal();
    }
}

fn handle_confirm_key(state: &mut TuiAppState, code: KeyCode, allow_exclude_secrets: bool) {
    match code {
        KeyCode::Enter | KeyCode::Char('y' | 'Y' | 's' | 'S') => {
            state.confirm_copy_anyway();
        }
        KeyCode::Char('x' | 'X') if allow_exclude_secrets => {
            state.exclude_active_secret_files();
        }
        KeyCode::Esc | KeyCode::Char('n' | 'N' | 'q' | 'Q') => state.close_modal(),
        _ => {}
    }
}
