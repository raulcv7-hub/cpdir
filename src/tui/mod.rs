//! Interactive terminal user interface built on `ratatui` and `crossterm`.

pub mod input;
pub mod state;
pub mod view;

pub use crate::domain::InspectorTab;
pub use input::{TuiViewportAreas, handle_key_event, handle_mouse_event};
pub use state::{AppExitAction, InspectorState, TreeNavigator, TuiAppState, UiMode};
pub use view::{
    NodeVisualKind, cursor_glyph_style, cursor_row_style, format_token_count, node_label_style,
    render_active_modal, render_active_modal_with_layout, render_tree_view,
    render_tree_view_with_layout, sanitize_tui_label, secret_marker_style, secret_rule_label,
    total_tokens_style, tree_guide_style,
};
