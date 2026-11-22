//! Coordinated TUI state transitions spanning the domain tree, navigator, and inspector modal.

use crate::domain::{
    DirectoryTree, InspectorEntry, InspectorEntryKind, InspectorTab, MergeSubtreeOutcome, NodeId,
    NodeKind,
};
use crate::tui::state::app_state::{ActiveModal, TuiAppState};
use crate::tui::state::inspector::InspectorState;

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
        if let Some(node) = self.tree.get(row.node_id)
            && let NodeKind::Directory(dir) = &node.kind
        {
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

    /// Opens the unified inspector modal directly on `tab`.
    pub fn open_inspector_tab(&mut self, tab: InspectorTab) {
        let prev_vh = match &self.modal {
            ActiveModal::Inspector(ins) => ins.viewport_height(),
            _ => 14,
        };
        let mut next = InspectorState::new(&self.tree, self.metrics(), tab);
        next.set_viewport_height(prev_vh);
        self.modal = ActiveModal::Inspector(next);
    }

    /// Cycles the active inspector tab forward (`forward = true`) or backward.
    pub fn cycle_inspector_tab(&mut self, forward: bool) {
        let current = match &self.modal {
            ActiveModal::Inspector(ins) => ins.tab(),
            _ => return,
        };
        let next = if forward {
            current.next()
        } else {
            current.prev()
        };
        self.open_inspector_tab(next);
    }

    /// Sets the highlighted entry index inside the active inspector modal.
    pub fn set_inspector_cursor(&mut self, cursor: usize) {
        if let ActiveModal::Inspector(ins) = &mut self.modal {
            ins.set_cursor(cursor);
        }
    }

    /// Moves the active inspector cursor by `delta` rows (`isize`).
    pub fn scroll_inspector(&mut self, delta: isize) {
        if let ActiveModal::Inspector(ins) = &mut self.modal {
            ins.move_cursor(delta);
        }
    }

    /// Jumps the inspector cursor to the first item.
    pub const fn inspector_home(&mut self) {
        if let ActiveModal::Inspector(ins) = &mut self.modal {
            ins.move_home();
        }
    }

    /// Jumps the inspector cursor to the last item.
    pub fn inspector_end(&mut self) {
        if let ActiveModal::Inspector(ins) = &mut self.modal {
            ins.move_end();
        }
    }

    /// Toggles selection in `Unread`/`Secrets`, or requests re-ingestion for `[PRUNED]` dirs.
    pub fn toggle_inspector_selection(&mut self) {
        enum Action {
            ToggleNode(NodeId),
            ReingestPruned(String),
            RejectNonText,
        }

        let action = match &self.modal {
            ActiveModal::Inspector(ins) => match ins.selected_entry() {
                Some(entry) => match &entry.kind {
                    InspectorEntryKind::UnreadDir { node_id, .. }
                    | InspectorEntryKind::UnreadFile { node_id, .. }
                    | InspectorEntryKind::SecretItem { node_id, .. } => {
                        Action::ToggleNode(*node_id)
                    }
                    InspectorEntryKind::DiscardedPrunedDir { .. } => {
                        Action::ReingestPruned(entry.rel_path.clone())
                    }
                    InspectorEntryKind::DiscardedFile { .. } => Action::RejectNonText,
                },
                None => return,
            },
            _ => return,
        };

        match action {
            Action::ToggleNode(node_id) => {
                self.tree.toggle_selection(node_id);
                self.refresh_metrics();
            }
            Action::ReingestPruned(rel_path) => {
                self.pending_reingest = Some(rel_path);
            }
            Action::RejectNonText => {
                self.set_inspector_status("Binary or invalid UTF-8 files cannot enter strict XML.");
            }
        }
    }

    /// Expands ancestors, moves the tree cursor to the inspected node, and closes the modal.
    pub fn jump_to_inspector_entry(&mut self) {
        let target = match &self.modal {
            ActiveModal::Inspector(ins) => ins.selected_entry().and_then(InspectorEntry::node_id),
            _ => None,
        };
        if let Some(node_id) = target {
            self.tree.expand_ancestors(node_id);
            self.nav.refresh(&self.tree);
            self.nav.jump_to_node(node_id);
            self.modal = ActiveModal::None;
        }
    }

    /// Returns and clears any pending pruned-directory re-scan request triggered from `Discarded`.
    pub fn take_pending_reingest(&mut self) -> Option<String> {
        self.pending_reingest.take()
    }

    /// Sets an informational status message inside the active inspector modal.
    pub fn set_inspector_status(&mut self, note: impl Into<String>) {
        if let ActiveModal::Inspector(ins) = &mut self.modal {
            ins.set_status_note(note);
        }
    }

    /// Merges a re-scanned pruned directory into the tree as collapsed and unselected by default.
    pub fn apply_reingested_dir(
        &mut self,
        rel_path: &str,
        subtree: DirectoryTree,
        truncated: bool,
    ) -> Option<MergeSubtreeOutcome> {
        let outcome = self.tree.merge_pruned_subtree(rel_path, subtree)?;
        self.refresh_metrics();
        self.nav.refresh(&self.tree);

        let note = if truncated {
            format!("Loaded {rel_path}/ (collapsed, unselected; truncated at entry cap)")
        } else {
            format!("Loaded {rel_path}/ (collapsed & unselected)")
        };

        let next_ins = if let ActiveModal::Inspector(ins) = &self.modal {
            let mut next = InspectorState::new(&self.tree, self.metrics(), ins.tab());
            next.set_viewport_height(ins.viewport_height());
            next.set_cursor(ins.cursor());
            next.set_status_note(note);
            Some(next)
        } else {
            None
        };
        if let Some(next) = next_ins {
            self.modal = ActiveModal::Inspector(next);
        }
        Some(outcome)
    }
}
