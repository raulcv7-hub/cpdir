//! Pure functional domain model for directory trees, selection state, and metrics.

pub mod findings;
pub mod git_snapshot;
pub mod inspector;
pub mod merge;
pub mod metrics;
pub mod operations;
pub mod overrides;
pub mod rollup;
pub mod selection;
pub mod tree;

pub use findings::{SecretFinding, SecretRuleKind};
pub use git_snapshot::{
    GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry, GitDiffStat, GitHotspotEntry,
    GitTagEntry,
};
pub use merge::MergeSubtreeOutcome;
pub use metrics::{
    CategoryCounts, InspectorCategory, InspectorItem, InspectorReason, NodeRollup, TreeMetrics,
    WeightTier,
};
pub use operations::VisibleRow;
pub use selection::{
    FileDiscardReason, FolderSelectionState, NaturalExclusion, NaturalSelection, PrunedDirReason,
    TextFileState, UnselectedReason,
};
pub use tree::{
    DirectoryData, DirectoryTree, NodeId, NodeKind, PrunedDir, TextFileData, TreeError, TreeNode,
};
