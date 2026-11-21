//! Pure functional domain model for directory trees, selection state, and metrics.

pub mod inspector;
pub mod metrics;
pub mod operations;
pub mod overrides;
pub mod rollup;
pub mod selection;
pub mod tree;

pub use metrics::{
    CategoryCounts, InspectorCategory, InspectorItem, InspectorReason, NodeRollup, TreeMetrics,
    WeightTier,
};
pub use operations::VisibleRow;
pub use selection::{
    FileDiscardReason, FolderSelectionState, NaturalExclusion, NaturalSelection, PrunedDirReason,
    SecretFinding, SecretRuleKind, TextFileState, UnselectedReason,
};
pub use tree::{
    DirectoryData, DirectoryTree, NodeId, NodeKind, PrunedDir, TextFileData, TreeError, TreeNode,
};
