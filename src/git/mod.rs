//! Hardened Git metadata extraction for `<git_context>`.

pub mod context;
mod runner;

pub use context::{GitContextConfig, extract_git_context};
