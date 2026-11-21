//! Hardened Git metadata extraction for `<git_context>`.

pub mod context;
mod parser;
mod runner;

pub use context::{GitContextConfig, extract_git_context, extract_git_context_status};
pub use runner::GitAvailability;
