//! Hardened Git metadata extraction for `<git_context>`.

pub mod context;
mod history_parser;
mod queries;
mod runner;
mod status_parser;

pub use context::{GitContextConfig, extract_git_context, extract_git_context_status};
pub use runner::GitAvailability;
