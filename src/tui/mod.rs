//! Interactive terminal user interface built on `ratatui` and `crossterm`.

pub mod app_state;
pub mod theme;

pub use app_state::{AppExitAction, TuiAppState, UiMode};
pub use theme::{
    GEMINI_TOKEN_TARGET, NodeVisualKind, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, secret_marker_style, total_tokens_style,
};
