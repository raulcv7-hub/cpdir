//! Terminal keyboard/mouse event dispatchers and viewport hit-test geometry.

pub mod keyboard;
pub mod layout;
pub mod mouse;

pub use keyboard::handle_key_event;
pub use layout::TuiViewportAreas;
pub use mouse::handle_mouse_event;
