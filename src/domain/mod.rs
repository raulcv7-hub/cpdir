//! Pure functional domain model for directory trees, selection state, and metrics.

pub mod findings;
pub mod folding;
pub mod git_snapshot;
pub mod inspector;
mod inspector_query;
pub mod merge;
pub mod metrics;
pub mod node;
pub mod operations;
pub mod overrides;
pub mod rollup;
pub mod selection;
pub mod tree;

pub use findings::{SecretFinding, SecretRuleKind};
pub use folding::VisibleRow;
pub use git_snapshot::{
    GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry, GitDiffStat, GitHotspotEntry,
    GitTagEntry,
};
pub use inspector::{InspectorEntry, InspectorEntryKind, InspectorTab};
pub use merge::MergeSubtreeOutcome;
pub use metrics::{CategoryCounts, NodeRollup, TreeMetrics, WeightTier, format_token_count};
pub use node::{DirectoryData, NodeId, NodeKind, PrunedDir, TextFileData, TreeError, TreeNode};
pub use selection::{
    FileDiscardReason, FolderSelectionState, NaturalExclusion, NaturalSelection, PrunedDirReason,
    TextFileState, UnselectedReason,
};
pub use tree::DirectoryTree;
