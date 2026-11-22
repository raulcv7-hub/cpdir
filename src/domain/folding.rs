//! Pure iterative tree expansion, ancestor reveal, and visible-row flattening.

use crate::domain::node::{NodeId, NodeKind};
use crate::domain::tree::DirectoryTree;

/// Flattened visible entry emitted for TUI rendering and cursor indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleRow {
    /// Arena handle of the rendered node.
    pub node_id: NodeId,
    /// Visual indentation depth (`0` for root).
    pub depth: usize,
}

impl DirectoryTree {
    /// Toggles expansion state on a directory node (`false` return if not a directory).
    pub fn toggle_expand(&mut self, id: NodeId) -> bool {
        if let Some(NodeKind::Directory(dir)) = self.nodes.get_mut(id.0).map(|n| &mut n.kind) {
            dir.is_expanded = !dir.is_expanded;
            return true;
        }
        false
    }

    /// Expands `id` (if a directory) and all ancestor directories up to root.
    pub fn expand_ancestors(&mut self, id: NodeId) {
        let mut curr = Some(id);
        while let Some(nid) = curr {
            let Some(node) = self.nodes.get_mut(nid.0) else {
                break;
            };
            if let NodeKind::Directory(dir) = &mut node.kind {
                dir.is_expanded = true;
            }
            curr = node.parent;
        }
    }

    /// Collapses every subdirectory while keeping the root directory expanded.
    pub fn collapse_all_subdirs(&mut self) {
        for (idx, node) in self.nodes.iter_mut().enumerate() {
            if let NodeKind::Directory(dir) = &mut node.kind {
                dir.is_expanded = idx == 0;
            }
        }
    }

    /// Expands every directory across the tree.
    pub fn expand_all_dirs(&mut self) {
        for node in &mut self.nodes {
            if let NodeKind::Directory(dir) = &mut node.kind {
                dir.is_expanded = true;
            }
        }
    }

    /// Flattens currently expanded nodes in pre-order for TUI rendering.
    #[must_use]
    pub fn visible_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![(Self::ROOT, 0usize)];

        while let Some((node_id, depth)) = stack.pop() {
            rows.push(VisibleRow { node_id, depth });
            if let NodeKind::Directory(dir) = &self.nodes[node_id.0].kind
                && dir.is_expanded
            {
                for &child in dir.children.iter().rev() {
                    stack.push((child, depth + 1));
                }
            }
        }
        rows
    }
}
