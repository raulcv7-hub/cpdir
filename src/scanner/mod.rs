//! Filesystem traversal, ignore evaluation, and safe `UTF-8` ingestion.

pub mod content_store;
mod dir_entries;
pub mod file_reader;
pub mod ignore_rules;
pub mod rescan;
mod traversal;
pub mod walker;

pub use content_store::{
    ContentHydrationError, INLINE_CONTENT_CACHE_BYTES, hydrate_selected_contents,
    release_large_unselected_contents,
};
pub use file_reader::{ReadFileOutcome, read_bounded_utf8};
pub use ignore_rules::{IgnoreDecision, IgnoreRuleStack};
pub use rescan::{DEFAULT_RESCAN_MAX_ENTRIES, RescanOutcome, rescan_pruned_dir};
pub(crate) use traversal::DiscoveredFile;
pub(crate) use walker::discover_workspace_entries;
pub use walker::{ScanConfig, ScanError, ScanResult, scan_directory};
