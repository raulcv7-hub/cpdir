//! Interactive terminal user interface built on `ratatui` and `crossterm`.

pub mod input;
mod render_dialogs;
mod render_inspector_row;
pub mod render_modals;
mod render_row;
pub mod render_tree;
pub mod state;
pub mod theme;

pub use crate::domain::InspectorTab;
pub use input::{TuiViewportAreas, handle_key_event, handle_mouse_event};
pub use render_modals::{render_active_modal, render_active_modal_with_layout};
pub use render_tree::{render_tree_view, render_tree_view_with_layout};
pub use state::{AppExitAction, InspectorState, TreeNavigator, TuiAppState, UiMode};
pub use theme::{
    NodeVisualKind, cursor_glyph_style, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, secret_marker_style, secret_rule_label, total_tokens_style,
    tree_guide_style,
};
