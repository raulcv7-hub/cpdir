//! Core library crate for `cpdir`.

pub mod domain;

/// Returns the package version embedded at compile time.
#[must_use]
pub const fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
