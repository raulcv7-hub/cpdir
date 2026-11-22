//! Viewport cursor movement, scroll clamping, and directory folding transitions.

use crate::domain::{DirectoryTree, NodeId, NodeKind};
use crate::tui::app_state::TuiAppState;

impl TuiAppState {
    /// Moves the tree cursor up by one row.
    pub fn move_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Moves the tree cursor down by one row.
    pub fn move_down(&mut self) {
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = (self.cursor + 1).min(max_idx);
        }
    }

    /// Moves the tree cursor to the first visible row.
    pub const fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the tree cursor to the last visible row.
    pub fn move_end(&mut self) {
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = max_idx;
        }
    }

    /// Moves the tree cursor up by `step` rows.
    pub fn page_up(&mut self, step: usize) {
        self.cursor = self.cursor.saturating_sub(step);
    }

    /// Moves the tree cursor down by `step` rows.
    pub fn page_down(&mut self, step: usize) {
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = self.cursor.saturating_add(step).min(max_idx);
        }
    }

    /// Keeps `scroll_offset` aligned so `cursor` stays inside `viewport_height` with scrolloff.
    pub fn ensure_cursor_visible(&mut self, viewport_height: usize) {
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

    /// Toggles fold/unfold on the directory under the cursor.
    pub fn toggle_expand_at_cursor(&mut self) {
        if let Some(row) = self.visible_rows.get(self.cursor)
            && self.tree.toggle_expand(row.node_id)
        {
            self.refresh_visible_rows();
        }
    }

    /// Toggles fold/unfold on the directory at `visible_idx`.
    pub fn toggle_expand_at_index(&mut self, visible_idx: usize) {
        if let Some(row) = self.visible_rows.get(visible_idx)
            && self.tree.toggle_expand(row.node_id)
        {
            self.refresh_visible_rows();
        }
    }

    /// Collapses every subdirectory while keeping root expanded.
    pub fn collapse_all(&mut self) {
        self.tree.collapse_all_subdirs();
        self.refresh_visible_rows();
    }

    /// Expands every directory across the tree.
    pub fn expand_all(&mut self) {
        self.tree.expand_all_dirs();
        self.refresh_visible_rows();
    }

    /// Expands a collapsed directory under the cursor, or steps into its first child.
    pub fn expand_or_enter_at_cursor(&mut self) {
        let Some(row) = self.visible_rows.get(self.cursor).copied() else {
            return;
        };
        if let Some(node) = self.tree.get(row.node_id)
            && let NodeKind::Directory(dir) = &node.kind
        {
            if !dir.is_expanded {
                self.tree.toggle_expand(row.node_id);
                self.refresh_visible_rows();
            } else if !dir.children.is_empty() {
                self.move_down();
            }
        }
    }

    /// Collapses an expanded directory under the cursor, or jumps cursor to its parent folder.
    pub fn collapse_or_jump_parent(&mut self) {
        let Some(row) = self.visible_rows.get(self.cursor).copied() else {
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
            self.refresh_visible_rows();
            return;
        }
        if let Some(parent_id) = node.parent {
            self.jump_to_node(parent_id);
        }
    }

    pub(super) fn jump_to_node(&mut self, target: NodeId) {
        if let Some(pos) = self
            .visible_rows
            .iter()
            .position(|row| row.node_id == target)
        {
            self.cursor = pos;
        }
    }

    pub(super) fn refresh_visible_rows(&mut self) {
        self.visible_rows = self.tree.visible_rows();
        if let Some(max_idx) = self.visible_rows.len().checked_sub(1) {
            self.cursor = self.cursor.min(max_idx);
            self.scroll_offset = self.scroll_offset.min(self.cursor);
        } else {
            self.cursor = 0;
            self.scroll_offset = 0;
        }
    }
}
