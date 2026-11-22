//! Interactive `TestBackend` workflow test suite across tree, inspector, mouse, and fuzzing.

#[path = "../common/mod.rs"]
mod common;

mod inspector_reingest;
mod inspector_tabs;
mod mouse;
mod proptest_fuzz;
mod secrets_and_dialogs;
mod tree_view;
