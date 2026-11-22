//! Core library crate for `cpdir`.

pub mod application;
pub mod cli;
pub mod config;
pub mod domain;
pub mod git;
pub mod output;
pub mod scanner;
pub mod security;
pub mod state;
pub mod tokens;
pub mod tui;

/// Returns the package version embedded at compile time.
#[must_use]
pub const fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
