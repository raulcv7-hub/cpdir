//! Pure Ratatui presentation modules reading immutable [`TuiAppState`].

mod badges;
mod confirm_modals;
mod help_modal;
mod inspector_modal;
mod inspector_row;
mod inspector_tabs;
mod text_fit;
pub mod theme;
mod tree_chrome;
mod tree_row;
pub mod tree_view;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::tui::input::layout::TuiViewportAreas;
use crate::tui::state::{TuiAppState, UiMode};
use confirm_modals::{render_confirm_empty_modal, render_confirm_secrets_modal};
use help_modal::render_help_modal;
use inspector_modal::render_inspector_modal;

pub use theme::{
    NodeVisualKind, cursor_glyph_style, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, secret_marker_style, secret_rule_label, total_tokens_style,
    tree_guide_style,
};
pub use tree_view::{render_tree_view, render_tree_view_with_layout};

/// Renders the active modal overlay without retaining hit-boxes when `mode` is not [`UiMode::Tree`].
pub fn render_active_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    render_active_modal_with_layout(frame, area, state, &mut TuiViewportAreas::default());
}

/// Renders the active modal overlay on top of the main tree view, recording `layout`.
pub fn render_active_modal_with_layout(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiAppState,
    layout: &mut TuiViewportAreas,
) {
    match state.mode() {
        UiMode::Tree => {}
        UiMode::Inspector { tab, .. } => render_inspector_modal(frame, area, state, layout, tab),
        UiMode::Help => render_help_modal(frame, area, state),
        UiMode::ConfirmEmptyCopy => render_confirm_empty_modal(frame, area),
        UiMode::ConfirmSecrets {
            active_secret_files,
        } => {
            render_confirm_secrets_modal(frame, area, state, active_secret_files);
        }
    }
}
