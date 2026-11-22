//! Single source of truth for Unread, Discarded, and Secrets inspector queries.

use crate::domain::findings::SecretRuleKind;
use crate::domain::metrics::TreeMetrics;
use crate::domain::node::{NodeId, NodeKind};
use crate::domain::selection::{FileDiscardReason, PrunedDirReason, UnselectedReason};
use crate::domain::tree::DirectoryTree;

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
        /// Directory node handle inside [`DirectoryTree`].
        node_id: NodeId,
        /// Number of unselected readable text files inside this subtree.
        unread_count: usize,
        /// Total tokens across the unselected readable text files inside this subtree.
        tokens: usize,
    },
    /// Readable text file omitted from `<files>` for `reason`.
    UnreadFile {
        /// File node handle inside [`DirectoryTree`].
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
        /// File node handle inside [`DirectoryTree`].
        node_id: NodeId,
        /// Reason the file could not be ingested as `UTF-8` text.
        reason: FileDiscardReason,
    },
    /// Secret alert row referencing a flagged text file and optional line/rule detail.
    SecretItem {
        /// File node handle inside [`DirectoryTree`].
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
    /// Returns the underlying tree node handle if this entry maps to a node inside [`DirectoryTree`].
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

impl DirectoryTree {
    /// Collects inspector entries for `tab` using precomputed `metrics`.
    #[must_use]
    pub fn inspector_entries(
        &self,
        metrics: &TreeMetrics,
        tab: InspectorTab,
    ) -> Vec<InspectorEntry> {
        match tab {
            InspectorTab::Unread => collect_unread_entries(self, metrics),
            InspectorTab::Discarded => collect_discarded_entries(self),
            InspectorTab::Secrets => collect_secret_entries(self),
        }
    }
}

fn collect_unread_entries(tree: &DirectoryTree, metrics: &TreeMetrics) -> Vec<InspectorEntry> {
    let mut entries = Vec::new();
    for (idx, node) in tree.nodes().iter().enumerate() {
        match &node.kind {
            NodeKind::Directory(_) if idx > 0 => {
                let rollup = metrics.rollups.get(idx).copied().unwrap_or_default();
                if rollup.selected_files == 0 && rollup.unread_files > 0 {
                    entries.push(InspectorEntry {
                        rel_path: node.rel_path.clone(),
                        kind: InspectorEntryKind::UnreadDir {
                            node_id: NodeId(idx),
                            unread_count: rollup.unread_files,
                            tokens: rollup.total_tokens,
                        },
                    });
                }
            }
            NodeKind::TextFile(file) => {
                if let Some(reason) = file.state.unselected_reason() {
                    entries.push(InspectorEntry {
                        rel_path: node.rel_path.clone(),
                        kind: InspectorEntryKind::UnreadFile {
                            node_id: NodeId(idx),
                            reason,
                            tokens: file.tokens,
                        },
                    });
                }
            }
            NodeKind::Directory(_) | NodeKind::DiscardedFile(_) => {}
        }
    }
    entries
}

fn collect_discarded_entries(tree: &DirectoryTree) -> Vec<InspectorEntry> {
    let mut entries = Vec::new();
    for pruned in tree.pruned_dirs() {
        entries.push(InspectorEntry {
            rel_path: pruned.rel_path.clone(),
            kind: InspectorEntryKind::DiscardedPrunedDir {
                reason: pruned.reason,
            },
        });
    }
    for (idx, node) in tree.nodes().iter().enumerate() {
        if let NodeKind::DiscardedFile(reason) = node.kind {
            entries.push(InspectorEntry {
                rel_path: node.rel_path.clone(),
                kind: InspectorEntryKind::DiscardedFile {
                    node_id: NodeId(idx),
                    reason,
                },
            });
        }
    }
    entries
}

fn collect_secret_entries(tree: &DirectoryTree) -> Vec<InspectorEntry> {
    let mut entries = Vec::new();
    for (idx, node) in tree.nodes().iter().enumerate() {
        if let NodeKind::TextFile(file) = &node.kind
            && file.secret_hits > 0
        {
            let node_id = NodeId(idx);
            let findings = tree.secret_findings(node_id);
            if findings.is_empty() {
                entries.push(InspectorEntry {
                    rel_path: node.rel_path.clone(),
                    kind: InspectorEntryKind::SecretItem {
                        node_id,
                        line: None,
                        rule: None,
                        hits: file.secret_hits,
                    },
                });
            } else {
                for finding in findings {
                    entries.push(InspectorEntry {
                        rel_path: node.rel_path.clone(),
                        kind: InspectorEntryKind::SecretItem {
                            node_id,
                            line: Some(finding.line),
                            rule: Some(finding.rule),
                            hits: file.secret_hits,
                        },
                    });
                }
            }
        }
    }
    entries
}
