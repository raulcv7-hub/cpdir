//! Pure category collector queries building [`InspectorEntry`] lists from [`DirectoryTree`].

use crate::domain::inspector::{InspectorEntry, InspectorEntryKind, InspectorTab};
use crate::domain::metrics::TreeMetrics;
use crate::domain::node::{NodeId, NodeKind};
use crate::domain::tree::DirectoryTree;

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
