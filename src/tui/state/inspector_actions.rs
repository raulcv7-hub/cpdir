//! Inspector modal navigation, toggling, jump-to-tree, and re-ingest transitions on [`TuiAppState`].

use crate::domain::{
    DirectoryTree, InspectorEntry, InspectorEntryKind, InspectorTab, MergeSubtreeOutcome, NodeId,
};
use crate::tui::state::app_state::TuiAppState;
use crate::tui::state::inspector::InspectorState;
use crate::tui::state::modes::ActiveModal;

impl TuiAppState {
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
        let ActiveModal::Inspector(ins) = &self.modal else {
            return;
        };
        let current = ins.tab();
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

    /// Toggles selection in `Unread`/`Secrets`, or requests re-ingestion for pruned dirs.
    pub fn toggle_inspector_selection(&mut self) {
        enum Action {
            ToggleNode(NodeId),
            ReingestPruned(String),
            RejectNonText,
        }

        let ActiveModal::Inspector(ins) = &self.modal else {
            return;
        };
        let Some(entry) = ins.selected_entry() else {
            return;
        };
        let action = match &entry.kind {
            InspectorEntryKind::UnreadDir { node_id, .. }
            | InspectorEntryKind::UnreadFile { node_id, .. }
            | InspectorEntryKind::SecretItem { node_id, .. } => Action::ToggleNode(*node_id),
            InspectorEntryKind::DiscardedPrunedDir { .. } => {
                Action::ReingestPruned(entry.rel_path.clone())
            }
            InspectorEntryKind::DiscardedFile { .. } => Action::RejectNonText,
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
        let ActiveModal::Inspector(ins) = &self.modal else {
            return;
        };
        let Some(node_id) = ins.selected_entry().and_then(InspectorEntry::node_id) else {
            return;
        };
        self.tree.expand_ancestors(node_id);
        self.nav.refresh(&self.tree);
        self.nav.jump_to_node(node_id);
        self.modal = ActiveModal::None;
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
            format!(
                "Loaded {rel_path}/ (collapsed, unselected; truncated at entry cap) - check [Unread] or [Esc]"
            )
        } else {
            format!("Loaded {rel_path}/ (collapsed & unselected) - check [Unread] or [Esc]")
        };

        if let ActiveModal::Inspector(ins) = &self.modal {
            let (tab, vh, cursor) = (ins.tab(), ins.viewport_height(), ins.cursor());
            let mut next = InspectorState::new(&self.tree, self.metrics(), tab);
            next.set_viewport_height(vh);
            next.set_cursor(cursor);
            next.set_status_note(note);
            self.modal = ActiveModal::Inspector(next);
        }
        Some(outcome)
    }
}
