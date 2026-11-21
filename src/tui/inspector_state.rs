//! State and in-place actions for the unified Unread / Discarded / Secrets inspector modal.

use crate::domain::{
    DirectoryTree, FileDiscardReason, InspectorCategory, NodeId, NodeKind, PrunedDirReason,
    SecretRuleKind, TreeMetrics, UnselectedReason,
};
use crate::tui::app_state::{ActiveModal, TuiAppState};

/// Active tab inside the unified inspector overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    /// Readable text files and folders currently unselected.
    Unread,
    /// Pruned dependency/VCS directories and discarded binary/corrupt/oversized files.
    Discarded,
    /// Files and lines flagged by the local heuristic secret scanner.
    Secrets,
}

impl InspectorTab {
    /// Cycles to the next inspector tab in display order.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Unread => Self::Discarded,
            Self::Discarded => Self::Secrets,
            Self::Secrets => Self::Unread,
        }
    }

    /// Cycles to the previous inspector tab in display order.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Unread => Self::Secrets,
            Self::Discarded => Self::Unread,
            Self::Secrets => Self::Discarded,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InspectorEntryKind {
    UnreadDir {
        node_id: NodeId,
        unread_count: usize,
        tokens: usize,
    },
    UnreadFile {
        node_id: NodeId,
        reason: UnselectedReason,
        tokens: usize,
    },
    DiscardedPrunedDir {
        reason: PrunedDirReason,
    },
    DiscardedFile {
        node_id: NodeId,
        reason: FileDiscardReason,
    },
    SecretItem {
        node_id: NodeId,
        line: Option<usize>,
        rule: Option<SecretRuleKind>,
        hits: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorEntry {
    pub(crate) rel_path: String,
    pub(crate) kind: InspectorEntryKind,
}

impl InspectorEntry {
    pub(crate) const fn node_id(&self) -> Option<NodeId> {
        match self.kind {
            InspectorEntryKind::UnreadDir { node_id, .. }
            | InspectorEntryKind::UnreadFile { node_id, .. }
            | InspectorEntryKind::DiscardedFile { node_id, .. }
            | InspectorEntryKind::SecretItem { node_id, .. } => Some(node_id),
            InspectorEntryKind::DiscardedPrunedDir { .. } => None,
        }
    }
}

/// Cached interactive state backing the unified inspector modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorState {
    tab: InspectorTab,
    entries: Vec<InspectorEntry>,
    cursor: usize,
    scroll: usize,
}

impl InspectorState {
    #[must_use]
    pub(crate) fn new(tree: &DirectoryTree, metrics: &TreeMetrics, tab: InspectorTab) -> Self {
        Self {
            tab,
            entries: collect_tab_entries(tree, metrics, tab),
            cursor: 0,
            scroll: 0,
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

    /// Toggles selection directly for the currently highlighted `Unread` or `Secrets` entry.
    pub fn toggle_inspector_selection(&mut self) {
        let target = match &self.modal {
            ActiveModal::Inspector(ins)
                if matches!(ins.tab(), InspectorTab::Unread | InspectorTab::Secrets) =>
            {
                ins.selected_entry().and_then(InspectorEntry::node_id)
            }
            _ => None,
        };
        if let Some(node_id) = target {
            self.tree.toggle_selection(node_id);
            self.refresh_metrics();
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

    pub(crate) const fn active_inspector(&self) -> Option<&InspectorState> {
        match &self.modal {
            ActiveModal::Inspector(ins) => Some(ins),
            _ => None,
        }
    }
}

fn collect_tab_entries(
    tree: &DirectoryTree,
    metrics: &TreeMetrics,
    tab: InspectorTab,
) -> Vec<InspectorEntry> {
    let mut entries = Vec::new();
    match tab {
        InspectorTab::Unread => {
            for (idx, node) in tree.nodes().iter().enumerate() {
                match &node.kind {
                    NodeKind::Directory(_) if idx > 0 => {
                        let rollup = metrics.rollups[idx];
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
        }
        InspectorTab::Discarded => {
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
        }
        InspectorTab::Secrets => {
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
        }
    }
    entries
}
