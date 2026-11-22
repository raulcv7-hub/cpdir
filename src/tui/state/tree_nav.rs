//! Encapsulated viewport cursor, scroll offset, and flattened visible-row tracking.

use crate::domain::{DirectoryTree, NodeId, VisibleRow};

/// Cursor and viewport scroll state over the currently expanded directory rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNavigator {
    visible_rows: Vec<VisibleRow>,
    cursor: usize,
    scroll_offset: usize,
}

impl TreeNavigator {
    /// Initializes navigation state at row `0` from `tree`.
    #[must_use]
    pub fn new(tree: &DirectoryTree) -> Self {
        Self {
            visible_rows: tree.visible_rows(),
            cursor: 0,
            scroll_offset: 0,
        }
    }

    /// Returns flattened visible rows matching currently expanded directories.
    #[must_use]
    pub fn visible_rows(&self) -> &[VisibleRow] {
        &self.visible_rows
    }

    /// Returns the zero-based cursor row inside [`Self::visible_rows`].
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the top visible row index of the tree viewport.
    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Returns the [`VisibleRow`] currently under the cursor, if any.
    #[must_use]
    pub fn current_row(&self) -> Option<VisibleRow> {
        self.visible_rows.get(self.cursor).copied()
    }

    /// Returns the [`VisibleRow`] at `idx`, if in bounds.
    #[must_use]
    pub fn row_at(&self, idx: usize) -> Option<VisibleRow> {
        self.visible_rows.get(idx).copied()
    }

    /// Sets cursor clamped to valid visible row bounds.
    pub fn set_cursor(&mut self, idx: usize) {
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = idx.min(max_idx);
        } else {
            self.cursor = 0;
        }
    }

    /// Moves the cursor up by one row.
    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Moves the cursor down by one row.
    pub fn move_down(&mut self) {
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = (self.cursor + 1).min(max_idx);
        }
    }

    /// Moves the cursor to the first visible row.
    pub const fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the cursor to the last visible row.
    pub fn move_end(&mut self) {
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = max_idx;
        }
    }

    /// Moves the cursor up by `step` rows.
    pub fn page_up(&mut self, step: usize) {
        self.cursor = self.cursor.saturating_sub(step);
    }

    /// Moves the cursor down by `step` rows.
    pub fn page_down(&mut self, step: usize) {
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = self.cursor.saturating_add(step).min(max_idx);
        }
    }

    /// Keeps `scroll_offset` aligned so `cursor` stays inside `viewport_height` with scrolloff.
    pub fn ensure_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        let margin = 2usize.min(viewport_height.saturating_sub(1) / 2);
        let max_scroll = self.visible_rows.len().saturating_sub(viewport_height);

        if self.cursor < self.scroll_offset.saturating_add(margin) {
            self.scroll_offset = self.cursor.saturating_sub(margin);
        } else if self.cursor >= self.scroll_offset + viewport_height - margin {
            self.scroll_offset = (self.cursor + margin + 1)
                .saturating_sub(viewport_height)
                .min(max_scroll);
        }
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    /// Moves the cursor to `target` if it is currently visible.
    pub fn jump_to_node(&mut self, target: NodeId) {
        if let Some(pos) = self
            .visible_rows
            .iter()
            .position(|row| row.node_id == target)
        {
            self.cursor = pos;
        }
    }

    /// Recomputes flattened visible rows after directory expansion changes and clamps cursor.
    pub fn refresh(&mut self, tree: &DirectoryTree) {
        self.visible_rows = tree.visible_rows();
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = self.cursor.min(max_idx);
            self.scroll_offset = self.scroll_offset.min(self.cursor);
        } else {
            self.cursor = 0;
            self.scroll_offset = 0;
        }
    }
}
