//! Local heuristic secret detection and exemption filtering.

pub mod secret_scan;

pub use secret_scan::{SecretFinding, SecretRuleKind, SecretScanner, populate_secret_hits};
