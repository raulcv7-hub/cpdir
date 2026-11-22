//! Tree viewport synchronization, folding, and selection transitions on [`TuiAppState`].

use crate::domain::{DirectoryTree, NodeKind};
use crate::tui::state::app_state::TuiAppState;
use crate::tui::state::modes::ActiveModal;

impl TuiAppState {
    /// Synchronizes tree and inspector scroll offsets against current rendered viewport heights.
    pub fn prepare_viewports(&mut self, tree_height: usize, inspector_height: usize) {
        self.nav.ensure_visible(tree_height);
        if let ActiveModal::Inspector(ins) = &mut self.modal {
            ins.ensure_visible(inspector_height);
        }
    }

    /// Toggles fold/unfold on the directory under the cursor.
    pub fn toggle_expand_at_cursor(&mut self) {
        if let Some(row) = self.nav.current_row()
            && self.tree.toggle_expand(row.node_id)
        {
            self.nav.refresh(&self.tree);
        }
    }

    /// Toggles fold/unfold on the directory at `visible_idx`.
    pub fn toggle_expand_at_index(&mut self, visible_idx: usize) {
        if let Some(row) = self.nav.row_at(visible_idx)
            && self.tree.toggle_expand(row.node_id)
        {
            self.nav.refresh(&self.tree);
        }
    }

    /// Collapses every subdirectory while keeping root expanded.
    pub fn collapse_all(&mut self) {
        self.tree.collapse_all_subdirs();
        self.nav.refresh(&self.tree);
    }

    /// Expands every directory across the tree.
    pub fn expand_all(&mut self) {
        self.tree.expand_all_dirs();
        self.nav.refresh(&self.tree);
    }

    /// Expands a collapsed directory under the cursor, or steps into its first child.
    pub fn expand_or_enter_at_cursor(&mut self) {
        let Some(row) = self.nav.current_row() else {
            return;
        };
        let Some(node) = self.tree.get(row.node_id) else {
            return;
        };
        if let NodeKind::Directory(dir) = &node.kind {
            if !dir.is_expanded {
                self.tree.toggle_expand(row.node_id);
                self.nav.refresh(&self.tree);
            } else if !dir.children.is_empty() {
                self.nav.move_down();
            }
        }
    }

    /// Collapses an expanded directory under the cursor, or jumps cursor to its parent folder.
    pub fn collapse_or_jump_parent(&mut self) {
        let Some(row) = self.nav.current_row() else {
            return;
        };
        let Some(node) = self.tree.get(row.node_id) else {
            return;
        };
        if let NodeKind::Directory(dir) = &node.kind
            && dir.is_expanded
            && row.node_id != DirectoryTree::ROOT
        {
            self.tree.toggle_expand(row.node_id);
            self.nav.refresh(&self.tree);
            return;
        }
        if let Some(parent_id) = node.parent {
            self.nav.jump_to_node(parent_id);
        }
    }

    /// Toggles selection for the file or subtree under the cursor.
    pub fn toggle_selection_at_cursor(&mut self) {
        if let Some(row) = self.nav.current_row() {
            self.tree.toggle_selection(row.node_id);
            self.refresh_metrics();
        }
    }

    /// Toggles between deselecting all files (when at clean defaults) and restoring safe defaults.
    pub fn toggle_all_selection(&mut self) {
        if self.metrics().counts.read_files > 0 && !self.tree.has_user_overrides() {
            self.unselect_all();
        } else {
            self.select_all_default();
        }
    }

    /// Resets all text files to safe natural selection defaults (`Select All`).
    pub fn select_all_default(&mut self) {
        self.tree.select_all_default();
        self.refresh_metrics();
    }

    /// Deselects all readable text files (`Unselect All`).
    pub fn unselect_all(&mut self) {
        self.tree.unselect_all();
        self.refresh_metrics();
    }

    /// Deselects all currently selected files carrying secret findings and returns to tree.
    pub fn exclude_active_secret_files(&mut self) {
        self.tree.unselect_secret_files();
        self.refresh_metrics();
        self.modal = ActiveModal::None;
    }
}
