//! Pure TUI state machines decoupled from `ratatui` rendering and `crossterm` events.

pub mod app_state;
mod context_tokens;
pub mod inspector;
mod inspector_actions;
pub mod modes;
mod tree_actions;
pub mod tree_nav;

pub use app_state::TuiAppState;
pub use inspector::InspectorState;
pub use modes::{AppExitAction, LoadingPhase, LoadingProgress, UiMode};
pub use tree_nav::TreeNavigator;
