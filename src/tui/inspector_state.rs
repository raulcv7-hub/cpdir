//! State and in-place actions for the unified Unread / Discarded / Secrets inspector modal.

use crate::domain::{DirectoryTree, InspectorCategory, MergeSubtreeOutcome, TreeMetrics};
use crate::tui::app_state::{ActiveModal, TuiAppState};
use crate::tui::inspector_items::{
    InspectorEntry, InspectorEntryKind, InspectorTab, collect_tab_entries,
};

/// Cached interactive state backing the unified inspector modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorState {
    tab: InspectorTab,
    entries: Vec<InspectorEntry>,
    cursor: usize,
    scroll: usize,
    status_note: Option<String>,
}

impl InspectorState {
    #[must_use]
    pub(crate) fn new(tree: &DirectoryTree, metrics: &TreeMetrics, tab: InspectorTab) -> Self {
        Self {
            tab,
            entries: collect_tab_entries(tree, metrics, tab),
            cursor: 0,
            scroll: 0,
            status_note: None,
        }
    }

    #[must_use]
    pub(crate) const fn tab(&self) -> InspectorTab {
        self.tab
    }

    #[must_use]
    pub(crate) const fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub(crate) const fn scroll(&self) -> usize {
        self.scroll
    }

    #[must_use]
    pub(crate) fn entries(&self) -> &[InspectorEntry] {
        &self.entries
    }

    #[must_use]
    pub(crate) fn status_note(&self) -> Option<&str> {
        self.status_note.as_deref()
    }

    pub(crate) fn set_status_note(&mut self, note: impl Into<String>) {
        self.status_note = Some(note.into());
    }

    pub(crate) fn restore_cursor(&mut self, prev_cursor: usize) {
        if let Some(max_idx) = self.entries.len().checked_sub(1) {
            self.cursor = prev_cursor.min(max_idx);
            self.clamp_scroll(14);
        } else {
            self.cursor = 0;
            self.scroll = 0;
        }
    }

    pub(crate) fn move_cursor(&mut self, delta: isize) {
        let Some(max_idx) = self.entries.len().checked_sub(1) else {
            self.cursor = 0;
            self.scroll = 0;
            return;
        };
        self.cursor = if delta.is_negative() {
            self.cursor.saturating_sub(delta.unsigned_abs())
        } else {
            self.cursor
                .saturating_add(delta.cast_unsigned())
                .min(max_idx)
        };
        self.clamp_scroll(14);
    }

    pub(crate) const fn move_home(&mut self) {
        self.cursor = 0;
        self.scroll = 0;
    }

    pub(crate) fn move_end(&mut self) {
        if let Some(max_idx) = self.entries.len().checked_sub(1) {
            self.cursor = max_idx;
            self.clamp_scroll(14);
        }
    }

    pub(crate) fn selected_entry(&self) -> Option<&InspectorEntry> {
        self.entries.get(self.cursor)
    }

    fn clamp_scroll(&mut self, nominal_height: usize) {
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + nominal_height {
            self.scroll = self.cursor + 1 - nominal_height;
        }
    }
}

impl TuiAppState {
    /// Opens the unified inspector modal directly on `tab`.
    pub fn open_inspector_tab(&mut self, tab: InspectorTab) {
        self.modal = ActiveModal::Inspector(InspectorState::new(&self.tree, self.metrics(), tab));
    }

    /// Opens the unified inspector overlay mapped from [`InspectorCategory`].
    pub fn open_inspector(&mut self, category: InspectorCategory) {
        let tab = match category {
            InspectorCategory::Discarded => InspectorTab::Discarded,
            InspectorCategory::Unread => InspectorTab::Unread,
        };
        self.open_inspector_tab(tab);
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
            ToggleNode(crate::domain::NodeId),
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
            self.refresh_visible_rows();
            self.jump_to_node(node_id);
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
        self.refresh_visible_rows();

        let note = if truncated {
            format!("Loaded {rel_path}/ (collapsed, unselected; truncated at entry cap)")
        } else {
            format!("Loaded {rel_path}/ (collapsed & unselected)")
        };

        let next_ins = if let ActiveModal::Inspector(ins) = &self.modal {
            let mut next = InspectorState::new(&self.tree, self.metrics(), ins.tab());
            next.restore_cursor(ins.cursor());
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

    pub(crate) const fn active_inspector(&self) -> Option<&InspectorState> {
        match &self.modal {
            ActiveModal::Inspector(ins) => Some(ins),
            _ => None,
        }
    }
}
