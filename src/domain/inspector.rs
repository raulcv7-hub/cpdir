//! Inspector tab and entry models shared across domain queries and UI rendering.

use crate::domain::findings::SecretRuleKind;
use crate::domain::node::NodeId;
use crate::domain::selection::{FileDiscardReason, PrunedDirReason, UnselectedReason};

/// Active tab inside the unified inspector view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    /// Readable text files and folders currently unselected.
    Unread,
    /// Pruned dependency/VCS directories and discarded binary/corrupt/oversized files.
    Discarded,
    /// Files and lines flagged by the local heuristic secret scanner.
    Secrets,
}

impl InspectorTab {
    /// Cycles to the next inspector tab in display order.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Unread => Self::Discarded,
            Self::Discarded => Self::Secrets,
            Self::Secrets => Self::Unread,
        }
    }

    /// Cycles to the previous inspector tab in display order.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Unread => Self::Secrets,
            Self::Discarded => Self::Unread,
            Self::Secrets => Self::Discarded,
        }
    }
}

/// Classification and metadata attached to a single row inside [`InspectorTab`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorEntryKind {
    /// Directory containing only unselected readable files.
    UnreadDir {
        /// Directory node handle inside [`crate::domain::DirectoryTree`].
        node_id: NodeId,
        /// Number of unselected readable text files inside this subtree.
        unread_count: usize,
        /// Total tokens across the unselected readable text files inside this subtree.
        tokens: usize,
    },
    /// Readable text file omitted from `<files>` for `reason`.
    UnreadFile {
        /// File node handle inside [`crate::domain::DirectoryTree`].
        node_id: NodeId,
        /// Active reason the file is currently unselected.
        reason: UnselectedReason,
        /// Token count assigned to this file.
        tokens: usize,
    },
    /// Dependency or VCS directory pruned before tree ingestion.
    DiscardedPrunedDir {
        /// Reason the directory was pruned during scan.
        reason: PrunedDirReason,
    },
    /// Non-text, corrupt, or oversized file omitted from `<files>`.
    DiscardedFile {
        /// File node handle inside [`crate::domain::DirectoryTree`].
        node_id: NodeId,
        /// Reason the file could not be ingested as `UTF-8` text.
        reason: FileDiscardReason,
    },
    /// Secret alert row referencing a flagged text file and optional line/rule detail.
    SecretItem {
        /// File node handle inside [`crate::domain::DirectoryTree`].
        node_id: NodeId,
        /// One-based source line number when detailed findings exist.
        line: Option<usize>,
        /// Secret rule classification when detailed findings exist.
        rule: Option<SecretRuleKind>,
        /// Total findings recorded in the file.
        hits: usize,
    },
}

/// Unified entry emitted for inspector rendering and domain assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorEntry {
    /// Normalized forward-slash relative path inside the workspace.
    pub rel_path: String,
    /// Tab-specific variant and metadata.
    pub kind: InspectorEntryKind,
}

impl InspectorEntry {
    /// Returns the underlying tree node handle if this entry maps to a node inside the tree.
    #[must_use]
    pub const fn node_id(&self) -> Option<NodeId> {
        match self.kind {
            InspectorEntryKind::UnreadDir { node_id, .. }
            | InspectorEntryKind::UnreadFile { node_id, .. }
            | InspectorEntryKind::DiscardedFile { node_id, .. }
            | InspectorEntryKind::SecretItem { node_id, .. } => Some(node_id),
            InspectorEntryKind::DiscardedPrunedDir { .. } => None,
        }
    }
}
