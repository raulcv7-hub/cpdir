//! Filesystem traversal, ignore evaluation, and safe `UTF-8` ingestion.

pub mod file_reader;
pub mod ignore_rules;

pub use file_reader::{ReadFileOutcome, read_bounded_utf8};
pub use ignore_rules::{IgnoreDecision, IgnoreRuleStack};
