//! Interactive terminal user interface built on `ratatui` and `crossterm`.

pub mod app_state;
pub mod input;
mod inspector_state;
mod render_dialogs;
pub mod render_modals;
mod render_row;
pub mod render_tree;
pub mod theme;
mod tree_nav;

pub use app_state::{AppExitAction, TuiAppState, UiMode};
pub use input::handle_key_event;
pub use inspector_state::InspectorTab;
pub use render_modals::render_active_modal;
pub use render_tree::render_tree_view;
pub use theme::{
    NodeVisualKind, cursor_glyph_style, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, secret_marker_style, secret_rule_label, total_tokens_style,
    tree_guide_style,
};
