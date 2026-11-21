//! Entry models and category collectors for the unified inspector modal.

use crate::domain::{
    DirectoryTree, FileDiscardReason, NodeId, NodeKind, PrunedDirReason, SecretRuleKind,
    TreeMetrics, UnselectedReason,
};

/// Active tab inside the unified inspector overlay.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InspectorEntryKind {
    UnreadDir {
        node_id: NodeId,
        unread_count: usize,
        tokens: usize,
    },
    UnreadFile {
        node_id: NodeId,
        reason: UnselectedReason,
        tokens: usize,
    },
    DiscardedPrunedDir {
        reason: PrunedDirReason,
    },
    DiscardedFile {
        node_id: NodeId,
        reason: FileDiscardReason,
    },
    SecretItem {
        node_id: NodeId,
        line: Option<usize>,
        rule: Option<SecretRuleKind>,
        hits: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorEntry {
    pub(crate) rel_path: String,
    pub(crate) kind: InspectorEntryKind,
}

impl InspectorEntry {
    pub(crate) const fn node_id(&self) -> Option<NodeId> {
        match self.kind {
            InspectorEntryKind::UnreadDir { node_id, .. }
            | InspectorEntryKind::UnreadFile { node_id, .. }
            | InspectorEntryKind::DiscardedFile { node_id, .. }
            | InspectorEntryKind::SecretItem { node_id, .. } => Some(node_id),
            InspectorEntryKind::DiscardedPrunedDir { .. } => None,
        }
    }
}

pub(crate) fn collect_tab_entries(
    tree: &DirectoryTree,
    metrics: &TreeMetrics,
    tab: InspectorTab,
) -> Vec<InspectorEntry> {
    match tab {
        InspectorTab::Unread => collect_unread_entries(tree, metrics),
        InspectorTab::Discarded => collect_discarded_entries(tree),
        InspectorTab::Secrets => collect_secret_entries(tree),
    }
}

fn collect_unread_entries(tree: &DirectoryTree, metrics: &TreeMetrics) -> Vec<InspectorEntry> {
    let mut entries = Vec::new();
    for (idx, node) in tree.nodes().iter().enumerate() {
        match &node.kind {
            NodeKind::Directory(_) if idx > 0 => {
                let rollup = metrics.rollups[idx];
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
