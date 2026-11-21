//! Interactive terminal user interface built on `ratatui` and `crossterm`.

pub mod app_state;
pub mod input;
pub mod render_modals;
pub mod render_tree;
pub mod theme;

pub use app_state::{AppExitAction, TuiAppState, UiMode};
pub use input::handle_key_event;
pub use render_modals::render_active_modal;
pub use render_tree::render_tree_view;
pub use theme::{
    GEMINI_TOKEN_TARGET, NodeVisualKind, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, secret_marker_style, total_tokens_style,
};
