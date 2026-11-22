//! Encapsulated state for the unified Unread / Discarded / Secrets inspector modal.

use crate::domain::{DirectoryTree, InspectorEntry, InspectorTab, TreeMetrics};

/// Cached interactive list and scroll state backing the unified inspector modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorState {
    tab: InspectorTab,
    entries: Vec<InspectorEntry>,
    cursor: usize,
    scroll: usize,
    viewport_height: usize,
    status_note: Option<String>,
}

impl InspectorState {
    /// Collects initial entries for `tab` from `tree` and `metrics`.
    #[must_use]
    pub fn new(tree: &DirectoryTree, metrics: &TreeMetrics, tab: InspectorTab) -> Self {
        Self {
            tab,
            entries: tree.inspector_entries(metrics, tab),
            cursor: 0,
            scroll: 0,
            viewport_height: 14,
            status_note: None,
        }
    }

    /// Returns the active inspector tab.
    #[must_use]
    pub const fn tab(&self) -> InspectorTab {
        self.tab
    }

    /// Returns the highlighted zero-based entry index.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the top visible entry index inside the inspector viewport.
    #[must_use]
    pub const fn scroll(&self) -> usize {
        self.scroll
    }

    /// Returns all collected entries for the active tab.
    #[must_use]
    pub fn entries(&self) -> &[InspectorEntry] {
        &self.entries
    }

    /// Returns an optional status message shown at the bottom of the inspector.
    #[must_use]
    pub fn status_note(&self) -> Option<&str> {
        self.status_note.as_deref()
    }

    /// Returns the entry currently under the inspector cursor, if any.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&InspectorEntry> {
        self.entries.get(self.cursor)
    }

    /// Sets an informational status message inside the inspector modal.
    pub fn set_status_note(&mut self, note: impl Into<String>) {
        self.status_note = Some(note.into());
    }

    /// Synchronizes `scroll` against an explicit rendered `viewport_height`.
    pub fn ensure_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        self.viewport_height = viewport_height;
        self.clamp_scroll();
    }

    /// Moves cursor to `idx` clamped within available entries.
    pub fn set_cursor(&mut self, idx: usize) {
        if let Some(max_idx) = self.entries.len().checked_sub(1) {
            self.cursor = idx.min(max_idx);
            self.clamp_scroll();
        } else {
            self.cursor = 0;
            self.scroll = 0;
        }
    }

    /// Moves cursor by signed `delta` rows, clamping within bounds.
    pub fn move_cursor(&mut self, delta: isize) {
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
        self.clamp_scroll();
    }

    /// Jumps cursor and scroll back to the first entry.
    pub const fn move_home(&mut self) {
        self.cursor = 0;
        self.scroll = 0;
    }

    /// Jumps cursor to the final entry.
    pub fn move_end(&mut self) {
        if let Some(max_idx) = self.entries.len().checked_sub(1) {
            self.cursor = max_idx;
            self.clamp_scroll();
        }
    }

    pub(super) const fn viewport_height(&self) -> usize {
        self.viewport_height
    }

    pub(super) const fn set_viewport_height(&mut self, vh: usize) {
        self.viewport_height = vh;
    }

    fn clamp_scroll(&mut self) {
        let vh = self.viewport_height.max(1);
        let max_start = self.entries.len().saturating_sub(vh);
        self.scroll = self.scroll.min(max_start);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + vh {
            self.scroll = self.cursor + 1 - vh;
        }
    }
}
