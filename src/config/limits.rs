//! Numeric safety thresholds, cache bounds, and Git extraction caps.

/// Default token threshold above which readable files start deselected in the TUI.
pub const DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD: usize = 50_000;

/// Hard file size ceiling (`5 MiB`) preventing OOM or stalls on giant dumps and blobs.
pub const HARD_READ_BYTE_CAP: u64 = 5 * 1024 * 1024;

/// Initial probe size (`8 KiB`) used to reject binaries and corrupt encodings before full reads.
pub const FILE_HEADER_PROBE_BYTES: usize = 8 * 1024;

/// Maximum byte size retained inline in RAM for unselected files during a TUI session.
pub const INLINE_CONTENT_CACHE_BYTES: u64 = 64 * 1024;

/// Maximum total bytes of selected file text pinned in RAM during interactive browsing.
pub const MAX_SELECTED_INLINE_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// Maximum total bytes of small unselected files pinned in RAM during interactive browsing.
pub const MAX_UNSELECTED_INLINE_CACHE_BYTES: u64 = 8 * 1024 * 1024;

/// Maximum byte length for exact per-file XML wrapper re-tokenization during startup ingestion.
pub const INLINE_XML_DELTA_BODY_CAP_BYTES: usize = 64 * 1024;

/// Text size threshold (`512 KiB`) above which BPE counting splits work across parallel chunks.
pub const TOKEN_CHUNK_THRESHOLD_BYTES: usize = 512 * 1024;

/// Target byte length (`128 KiB`) per parallel BPE chunk for huge UTF-8 files.
pub const TOKEN_CHUNK_TARGET_BYTES: usize = 128 * 1024;

/// Hard entry cap applied when re-scanning a pruned dependency/output directory from the TUI.
pub const DEFAULT_RESCAN_MAX_ENTRIES: usize = 2_000;

/// Entry threshold above which an ignored directory without `.ignore` rules is pruned lazily.
pub const DEFAULT_IGNORED_DIR_PRUNE_THRESHOLD: usize = 1_000;

/// Maximum time in milliseconds allowed per read-only `git` query before aborting.
pub const GIT_COMMAND_TIMEOUT_MS: u64 = 4_000;

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

/// Default commit history window scanned to compute file hotspots.
pub const DEFAULT_GIT_HOTSPOT_COMMIT_WINDOW: usize = 100;

/// Default maximum `git status` lines rendered in `<working_tree>`.
pub const DEFAULT_GIT_MAX_WORKING_TREE_LINES: usize = 100;

/// Default maximum `git diff --numstat` lines rendered in `<working_diff_numstat>`.
pub const DEFAULT_GIT_MAX_DIFF_NUMSTAT_LINES: usize = 50;
