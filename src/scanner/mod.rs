//! Filesystem traversal, ignore evaluation, and safe `UTF-8` ingestion.

pub mod content_store;
pub mod file_reader;
pub mod ignore_rules;
pub mod walker;

pub use content_store::{
    ContentHydrationError, INLINE_CONTENT_CACHE_BYTES, hydrate_selected_contents,
    release_large_unselected_contents,
};
pub use file_reader::{ReadFileOutcome, read_bounded_utf8};
pub use ignore_rules::{IgnoreDecision, IgnoreRuleStack};
pub use walker::{ScanConfig, ScanError, ScanResult, scan_directory};
