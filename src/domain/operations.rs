//! Pure iterative tree operations: rollups, folding, selection toggles, and flattening.

use crate::domain::metrics::{
    CategoryCounts, InspectorCategory, InspectorItem, InspectorReason, NodeRollup, TreeMetrics,
};
use crate::domain::selection::{FolderSelectionState, NaturalSelection};
use crate::domain::tree::{DirectoryData, DirectoryTree, NodeId, NodeKind, TextFileData};

/// Flattened visible entry emitted for TUI rendering and cursor indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleRow {
    /// Arena handle of the rendered node.
    pub node_id: NodeId,
    /// Visual indentation depth (`0` for root).
    pub depth: usize,
}

impl DirectoryTree {
    /// Computes bottom-up node rollups and global counters in linear `O(N)` time.
    ///
    /// Relies on the arena invariant that every parent index precedes its children,
    /// avoiding recursion and stack overflow risk on deep Windows directory trees.
    #[must_use]
    pub fn compute_metrics(&self) -> TreeMetrics {
        let len = self.nodes.len();
        let mut rollups = vec![NodeRollup::default(); len];
        let mut counts = CategoryCounts {
            discarded_items: self.pruned_dirs.len(),
            ..CategoryCounts::default()
        };

        for (idx, node) in self.nodes.iter().enumerate().rev() {
            match &node.kind {
                NodeKind::TextFile(file) => {
                    let selected = file.state.is_selected();
                    let has_secret = file.secret_hits > 0;
                    let is_natural = matches!(file.state.natural(), NaturalSelection::Included);

                    rollups[idx] = NodeRollup {
                        selected_tokens: if selected { file.tokens } else { 0 },
                        total_tokens: file.tokens,
                        selected_files: usize::from(selected),
                        unread_files: usize::from(!selected),
                        discarded_files: 0,
                        folder_selection: FolderSelectionState::NoTextFiles,
                        has_active_secret: selected && has_secret,
                        has_any_secret: has_secret,
                        has_natural_included: is_natural,
                    };

                    if selected {
                        counts.read_files += 1;
                        counts.selected_tokens += file.tokens;
                        if has_secret {
                            counts.active_secret_files += 1;
                        }
                    } else {
                        counts.unread_files += 1;
                    }
                    if has_secret {
                        counts.total_secret_files += 1;
                    }
                }
                NodeKind::DiscardedFile(_) => {
                    rollups[idx].discarded_files = 1;
                    counts.discarded_items += 1;
                }
                NodeKind::Directory(dir) => {
                    let mut agg = NodeRollup::default();
                    for &child in &dir.children {
                        let child_r = rollups[child.index()];
                        agg.selected_tokens += child_r.selected_tokens;
                        agg.total_tokens += child_r.total_tokens;
                        agg.selected_files += child_r.selected_files;
                        agg.unread_files += child_r.unread_files;
                        agg.discarded_files += child_r.discarded_files;
                        agg.has_active_secret |= child_r.has_active_secret;
                        agg.has_any_secret |= child_r.has_any_secret;
                        agg.has_natural_included |= child_r.has_natural_included;
                    }
                    agg.folder_selection =
                        derive_folder_selection(agg.selected_files, agg.unread_files);
                    rollups[idx] = agg;
                }
            }
        }

        TreeMetrics { rollups, counts }
    }

    /// Toggles expansion state on a directory node (`false` return if not a directory).
    pub fn toggle_expand(&mut self, id: NodeId) -> bool {
        if let Some(NodeKind::Directory(dir)) = self.nodes.get_mut(id.0).map(|n| &mut n.kind) {
            dir.is_expanded = !dir.is_expanded;
            return true;
        }
        false
    }

    /// Toggles selection for a file or recursively for a directory subtree.
    ///
    /// Turning a deselected directory back on restores safe natural defaults (keeping
    /// `.gitignore`, `.ignore`, and `>50k` files excluded) unless the subtree contains
    /// zero naturally included files, in which case all files inside are forced on.
    pub fn toggle_selection(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(id.0) else {
            return;
        };

        if matches!(node.kind, NodeKind::TextFile(_)) {
            if let Some(NodeKind::TextFile(file)) = self.nodes.get_mut(id.0).map(|n| &mut n.kind) {
                file.state.toggle();
            }
            return;
        }

        if !matches!(node.kind, NodeKind::Directory(_)) {
            return;
        }

        let rollup = self.compute_metrics().rollups[id.0];
        let mut stack = vec![id];

        if rollup.selected_files > 0 {
            let is_root = id == Self::ROOT;
            while let Some(curr) = stack.pop() {
                let mark_folder_excluded = !(is_root && curr == Self::ROOT);
                self.mutate_subtree_step(
                    curr,
                    &mut stack,
                    |dir| {
                        if mark_folder_excluded {
                            dir.folder_override = Some(false);
                        }
                    },
                    |file| file.state.set_selected(false),
                );
            }
        } else if rollup.has_natural_included {
            while let Some(curr) = stack.pop() {
                self.mutate_subtree_step(
                    curr,
                    &mut stack,
                    |dir| dir.folder_override = None,
                    |file| file.state.reset_to_natural(),
                );
            }
        } else {
            while let Some(curr) = stack.pop() {
                self.mutate_subtree_step(
                    curr,
                    &mut stack,
                    |dir| dir.folder_override = None,
                    |file| file.state.set_selected(true),
                );
            }
        }
    }

    /// Resets every text file and directory override to safe natural defaults.
    pub fn select_all_default(&mut self) {
        for node in &mut self.nodes {
            match &mut node.kind {
                NodeKind::Directory(dir) => dir.folder_override = None,
                NodeKind::TextFile(file) => file.state.reset_to_natural(),
                NodeKind::DiscardedFile(_) => {}
            }
        }
    }

    /// Deselects every readable text file across the entire tree.
    pub fn unselect_all(&mut self) {
        for (idx, node) in self.nodes.iter_mut().enumerate() {
            match &mut node.kind {
                NodeKind::Directory(dir) => {
                    // Keep root unflagged so future top-level folders still auto-include.
                    dir.folder_override = if idx == 0 { None } else { Some(false) };
                }
                NodeKind::TextFile(file) => file.state.set_selected(false),
                NodeKind::DiscardedFile(_) => {}
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

    fn mutate_subtree_step<FD, FF>(
        &mut self,
        id: NodeId,
        stack: &mut Vec<NodeId>,
        mut on_dir: FD,
        mut on_file: FF,
    ) where
        FD: FnMut(&mut DirectoryData),
        FF: FnMut(&mut TextFileData),
    {
        match &mut self.nodes[id.0].kind {
            NodeKind::Directory(dir) => {
                on_dir(dir);
                stack.extend(dir.children.iter().copied());
            }
            NodeKind::TextFile(file) => on_file(file),
            NodeKind::DiscardedFile(_) => {}
        }
    }
}

const fn derive_folder_selection(selected: usize, unread: usize) -> FolderSelectionState {
    match (selected, unread) {
        (0, 0) => FolderSelectionState::NoTextFiles,
        (_, 0) => FolderSelectionState::AllSelected,
        (0, _) => FolderSelectionState::NoneSelected,
        (_, _) => FolderSelectionState::Partial,
    }
}
