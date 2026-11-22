//! Pure TUI state machines decoupled from `ratatui` rendering and `crossterm` events.

mod actions;
pub mod app_state;
pub mod inspector;
pub mod tree_nav;

pub use app_state::{AppExitAction, TuiAppState, UiMode};
pub use inspector::InspectorState;
pub use tree_nav::TreeNavigator;
