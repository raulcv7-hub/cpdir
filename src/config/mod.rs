//! Static configuration limits and classification rules.

pub mod limits;
pub mod rules;

/// Backwards-compatible facade exposing all static limits and classification rules.
pub mod defaults {
    pub use super::limits::*;
    pub use super::rules::*;
}
