//! Arena-preserving insertion of re-scanned pruned directory subtrees.

use std::cmp::Ordering;

use crate::domain::tree::{DirectoryTree, NodeId, NodeKind, PrunedDir, TreeNode};

/// Result emitted after merging a re-ingested pruned subtree into [`DirectoryTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeSubtreeOutcome {
    /// Handle assigned to the newly inserted directory node.
    pub inserted_dir: NodeId,
    /// Mapping from `(subtree_node_id, target_tree_node_id)` for remapping content buffers.
    pub id_map: Vec<(NodeId, NodeId)>,
}

impl DirectoryTree {
    /// Merges a re-scanned `subtree` at `pruned_rel_path` as collapsed and unselected by default.
    ///
    /// Preserves the arena parent-before-child invariant without shifting existing [`NodeId`]s.
    pub fn merge_pruned_subtree(
        &mut self,
        pruned_rel_path: &str,
        subtree: Self,
    ) -> Option<MergeSubtreeOutcome> {
        if subtree.nodes.is_empty() {
            return None;
        }
        let (parent_id, dir_name) = self.resolve_parent_and_name(pruned_rel_path)?;
        let base_idx = self.nodes.len();
        let inserted_dir = NodeId(base_idx);
        let mut id_map = Vec::with_capacity(subtree.nodes.len());

        for (offset, mut node) in subtree.nodes.into_iter().enumerate() {
            let old_id = NodeId(offset);
            let new_id = NodeId(base_idx + offset);
            id_map.push((old_id, new_id));

            remap_subtree_node(
                &mut node,
                offset,
                base_idx,
                parent_id,
                dir_name,
                pruned_rel_path,
            );
            self.nodes.push(node);
        }

        for (old_id, findings) in subtree.secret_findings {
            self.secret_findings
                .insert(NodeId(base_idx + old_id.index()), findings);
        }

        self.pruned_dirs
            .retain(|entry| entry.rel_path != pruned_rel_path);
        for nested_pruned in subtree.pruned_dirs {
            let full = if nested_pruned.rel_path.is_empty() {
                pruned_rel_path.to_owned()
            } else {
                format!("{pruned_rel_path}/{}", nested_pruned.rel_path)
            };
            self.pruned_dirs.push(PrunedDir {
                rel_path: full,
                reason: nested_pruned.reason,
            });
        }

        self.insert_dir_sorted(parent_id, inserted_dir, dir_name);
        Some(MergeSubtreeOutcome {
            inserted_dir,
            id_map,
        })
    }

    fn resolve_parent_and_name<'a>(&self, rel_path: &'a str) -> Option<(NodeId, &'a str)> {
        let (parent_rel, dir_name) = match rel_path.rsplit_once('/') {
            Some((p, name)) => (p, name),
            None => ("", rel_path),
        };
        if dir_name.is_empty() {
            return None;
        }
        if parent_rel.is_empty() {
            return Some((Self::ROOT, dir_name));
        }
        let idx = self
            .nodes
            .iter()
            .position(|n| n.rel_path == parent_rel && matches!(n.kind, NodeKind::Directory(_)))?;
        Some((NodeId(idx), dir_name))
    }

    fn insert_dir_sorted(&mut self, parent_id: NodeId, new_dir_id: NodeId, new_name: &str) {
        let lower_new = new_name.to_ascii_lowercase();
        let insert_pos = {
            let NodeKind::Directory(parent_dir) = &self.nodes[parent_id.0].kind else {
                return;
            };
            parent_dir
                .children
                .iter()
                .position(|&cid| {
                    let child = &self.nodes[cid.0];
                    if !matches!(child.kind, NodeKind::Directory(_)) {
                        return true;
                    }
                    let child_lower = child.name.to_ascii_lowercase();
                    match lower_new.cmp(&child_lower) {
                        Ordering::Less => true,
                        Ordering::Equal => new_name < child.name.as_str(),
                        Ordering::Greater => false,
                    }
                })
                .unwrap_or(parent_dir.children.len())
        };

        if let NodeKind::Directory(parent_dir) = &mut self.nodes[parent_id.0].kind {
            parent_dir.children.insert(insert_pos, new_dir_id);
        }
    }
}

fn remap_subtree_node(
    node: &mut TreeNode,
    offset: usize,
    base_idx: usize,
    parent_id: NodeId,
    dir_name: &str,
    pruned_rel_path: &str,
) {
    if offset == 0 {
        node.parent = Some(parent_id);
        dir_name.clone_into(&mut node.name);
        pruned_rel_path.clone_into(&mut node.rel_path);
    } else {
        node.parent = node.parent.map(|pid| NodeId(base_idx + pid.index()));
        node.rel_path = format!("{pruned_rel_path}/{}", node.rel_path);
    }

    match &mut node.kind {
        NodeKind::Directory(dir) => {
            dir.is_expanded = false;
            dir.folder_override = if offset == 0 { Some(false) } else { None };
            for cid in &mut dir.children {
                *cid = NodeId(base_idx + cid.index());
            }
        }
        NodeKind::TextFile(file) => file.state.set_selected(false),
        NodeKind::DiscardedFile(_) => {}
    }
}
