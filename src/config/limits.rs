//! Numeric safety thresholds, cache bounds, and Git extraction caps.

/// Default token threshold above which readable files start deselected in the TUI.
pub const DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD: usize = 50_000;

/// Hard file size ceiling (`5 MiB`) preventing OOM or stalls on giant dumps and blobs.
pub const HARD_READ_BYTE_CAP: u64 = 5 * 1024 * 1024;

/// Maximum byte size retained inline in RAM for unselected files during a TUI session.
pub const INLINE_CONTENT_CACHE_BYTES: u64 = 64 * 1024;

/// Hard entry cap applied when re-scanning a pruned dependency/output directory from the TUI.
pub const DEFAULT_RESCAN_MAX_ENTRIES: usize = 2_000;

/// Default maximum recent commits extracted for `<git_context>`.
pub const DEFAULT_GIT_MAX_COMMITS: usize = 15;

/// Default maximum commit body characters before ellipsis truncation.
pub const DEFAULT_GIT_MAX_COMMIT_BODY_CHARS: usize = 300;

/// Default maximum recent Git tags extracted for `<git_context>`.
pub const DEFAULT_GIT_MAX_TAGS: usize = 5;

/// Default maximum tag summary characters before ellipsis truncation.
pub const DEFAULT_GIT_MAX_TAG_SUMMARY_CHARS: usize = 220;

/// Default maximum hotspot files reported in `<hotspots>`.
pub const DEFAULT_GIT_MAX_HOTSPOTS: usize = 10;

/// Default commit history window scanned to calculate file hotspots.
pub const DEFAULT_GIT_HOTSPOT_COMMIT_WINDOW: usize = 100;

/// Default maximum `git status` lines rendered in `<working_tree>`.
pub const DEFAULT_GIT_MAX_WORKING_TREE_LINES: usize = 100;

/// Default maximum `git diff --numstat` lines rendered in `<working_diff_numstat>`.
pub const DEFAULT_GIT_MAX_DIFF_NUMSTAT_LINES: usize = 50;
