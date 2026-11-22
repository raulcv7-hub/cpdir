//! Local heuristic secret detection and exemption filtering.

pub mod batch;
pub mod placeholders;
pub mod rules;
pub mod scanner;

pub use crate::domain::{SecretFinding, SecretRuleKind};
pub use batch::populate_secret_hits;
pub use scanner::SecretScanner;
