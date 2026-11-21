//! Hardened Git metadata extraction for `<git_context>`.

pub mod context;

pub use context::{GitContextConfig, extract_git_context};
