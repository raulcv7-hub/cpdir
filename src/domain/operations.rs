//! Pure iterative recursive selection toggles across [`DirectoryTree`].

use crate::domain::node::{DirectoryData, NodeId, NodeKind, TextFileData};
use crate::domain::tree::DirectoryTree;

impl DirectoryTree {
    /// Deselects all currently selected files that carry secret findings.
    pub fn unselect_secret_files(&mut self) {
        for node in &mut self.nodes {
            if let NodeKind::TextFile(file) = &mut node.kind
                && file.secret_hits > 0
                && file.state.is_selected()
            {
                file.state.set_selected(false);
            }
        }
    }

    /// Returns `true` when any directory or text file carries an explicit user override.
    #[must_use]
    pub fn has_user_overrides(&self) -> bool {
        self.nodes.iter().any(|node| match &node.kind {
            NodeKind::Directory(dir) => dir.folder_override.is_some(),
            NodeKind::TextFile(file) => file.state.user_override().is_some(),
            NodeKind::DiscardedFile(_) => false,
        })
    }

    /// Toggles selection for a file or recursively for a directory subtree.
    ///
    /// Turning a deselected directory back on restores safe natural defaults (keeping
    /// `.gitignore`, `.ignore`, `>50k`, and re-ingested pruned files excluded) unless the
    /// subtree contains zero naturally included files, in which case all files are forced on.
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
        if id == Self::ROOT {
            if rollup.selected_files > 0 {
                self.unselect_all();
                return;
            }
            if rollup.has_natural_included {
                self.select_all_default();
                return;
            }
        }

        self.toggle_subtree(id, rollup.selected_files > 0, rollup.has_natural_included);
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

    fn toggle_subtree(&mut self, id: NodeId, turn_off: bool, has_natural_included: bool) {
        let ancestor_excluded = id != Self::ROOT && self.is_ancestor_excluded(id);
        let root_override = if turn_off {
            Some(false)
        } else if ancestor_excluded {
            Some(true)
        } else {
            None
        };

        let mut stack = vec![id];
        while let Some(curr) = stack.pop() {
            let dir_override = if curr == id { root_override } else { None };
            self.mutate_subtree_step(
                curr,
                &mut stack,
                |dir| dir.folder_override = dir_override,
                |file| {
                    if turn_off {
                        file.state.set_selected(false);
                    } else if has_natural_included {
                        file.state.reset_to_natural();
                    } else {
                        file.state.set_selected(true);
                    }
                },
            );
        }
    }

    fn is_ancestor_excluded(&self, id: NodeId) -> bool {
        let mut curr = self.nodes.get(id.0).and_then(|n| n.parent);
        while let Some(pid) = curr {
            let Some(node) = self.nodes.get(pid.0) else {
                break;
            };
            if let NodeKind::Directory(dir) = &node.kind {
                match dir.folder_override {
                    Some(false) => return true,
                    Some(true) => return false,
                    None => {}
                }
            }
            curr = node.parent;
        }
        false
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
