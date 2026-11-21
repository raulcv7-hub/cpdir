//! Category inspector queries collecting Discarded and Unread entries with reasons.

use crate::domain::metrics::{InspectorCategory, InspectorItem, InspectorReason};
use crate::domain::tree::{DirectoryTree, NodeKind};

impl DirectoryTree {
    /// Collects items matching the requested inspector category (`Discarded` or `Unread`).
    #[must_use]
    pub fn inspector_items(&self, category: InspectorCategory) -> Vec<InspectorItem> {
        let mut items = Vec::new();
        match category {
            InspectorCategory::Discarded => {
                for pruned in &self.pruned_dirs {
                    items.push(InspectorItem {
                        rel_path: pruned.rel_path.clone(),
                        tokens: 0,
                        reason: InspectorReason::PrunedDir(pruned.reason),
                    });
                }
                for node in &self.nodes {
                    if let NodeKind::DiscardedFile(reason) = node.kind {
                        items.push(InspectorItem {
                            rel_path: node.rel_path.clone(),
                            tokens: 0,
                            reason: InspectorReason::DiscardedFile(reason),
                        });
                    }
                }
            }
            InspectorCategory::Unread => {
                for node in &self.nodes {
                    if let NodeKind::TextFile(file) = &node.kind
                        && let Some(reason) = file.state.unselected_reason()
                    {
                        items.push(InspectorItem {
                            rel_path: node.rel_path.clone(),
                            tokens: file.tokens,
                            reason: InspectorReason::UnreadFile(reason),
                        });
                    }
                }
            }
        }
        items
    }
}
