//! Pure TUI state machine driving tree navigation, modals, and exit intents.

use crate::domain::{
    DirectoryTree, InspectorCategory, InspectorItem, NodeId, NodeKind, TreeMetrics, VisibleRow,
};

/// Active view layer controlling key handling and modal overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// Primary interactive directory tree view.
    Tree,
    /// Scrollable inspector listing either Discarded or Unread entries.
    Inspector {
        /// Category currently displayed.
        category: InspectorCategory,
        /// Top visible row offset inside the modal list.
        scroll: usize,
    },
    /// Confirmation prompt displayed on `Enter` when selected files contain potential secrets.
    ConfirmSecrets {
        /// Number of selected files carrying `[!]` warnings.
        active_secret_files: usize,
    },
}

/// Terminal outcome emitted when the interactive event loop terminates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppExitAction {
    /// Render strict XML and copy it to the Windows clipboard.
    CopyToClipboard,
    /// Exit cleanly without modifying the clipboard.
    Cancelled,
}

/// Pure UI state holding the directory tree, cached metrics, cursor, and modal modes.
#[derive(Debug, Clone)]
pub struct TuiAppState {
    tree: DirectoryTree,
    metrics: TreeMetrics,
    visible_rows: Vec<VisibleRow>,
    cursor: usize,
    scroll_offset: usize,
    has_git_context: bool,
    include_git_context: bool,
    mode: UiMode,
    exit_action: Option<AppExitAction>,
}

impl TuiAppState {
    /// Initializes TUI state from a scanned and tokenized [`DirectoryTree`].
    #[must_use]
    pub fn new(tree: DirectoryTree, has_git_context: bool) -> Self {
        let metrics = tree.compute_metrics();
        let visible_rows = tree.visible_rows();
        Self {
            tree,
            metrics,
            visible_rows,
            cursor: 0,
            scroll_offset: 0,
            has_git_context,
            include_git_context: has_git_context,
            mode: UiMode::Tree,
            exit_action: None,
        }
    }

    /// Returns an immutable reference to the underlying directory tree.
    #[must_use]
    pub const fn tree(&self) -> &DirectoryTree {
        &self.tree
    }

    /// Consumes the UI state and yields the updated [`DirectoryTree`] (for persistence/XML).
    #[must_use]
    pub fn into_tree(self) -> DirectoryTree {
        self.tree
    }

    /// Returns cached bottom-up metrics and global counters.
    #[must_use]
    pub const fn metrics(&self) -> &TreeMetrics {
        &self.metrics
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

    /// Returns `true` when the workspace is inside a valid Git repository.
    #[must_use]
    pub const fn has_git_context(&self) -> bool {
        self.has_git_context
    }

    /// Returns `true` when `<git_context>` will be included in the copied XML.
    #[must_use]
    pub const fn include_git_context(&self) -> bool {
        self.has_git_context && self.include_git_context
    }

    /// Overrides the initial Git toggle state (e.g., from persisted directory settings).
    pub fn set_include_git_context(&mut self, enabled: bool) {
        if self.has_git_context {
            self.include_git_context = enabled;
        }
    }

    /// Returns the current UI mode (main tree or active modal).
    #[must_use]
    pub const fn mode(&self) -> UiMode {
        self.mode
    }

    /// Returns the exit decision once the user confirms or cancels.
    #[must_use]
    pub const fn exit_action(&self) -> Option<AppExitAction> {
        self.exit_action
    }

    /// Collects inspector items for the requested category.
    #[must_use]
    pub fn inspector_items(&self, category: InspectorCategory) -> Vec<InspectorItem> {
        self.tree.inspector_items(category)
    }

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

    /// Keeps `scroll_offset` aligned so `cursor` stays inside `viewport_height`.
    pub fn ensure_cursor_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else if self.cursor >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.cursor + 1 - viewport_height;
        }
    }

    /// Toggles fold/unfold on the directory under the cursor.
    pub fn toggle_expand_at_cursor(&mut self) {
        if let Some(row) = self.visible_rows.get(self.cursor)
            && self.tree.toggle_expand(row.node_id)
        {
            self.refresh_visible_rows();
        }
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

    /// Toggles selection for the file or subtree under the cursor.
    pub fn toggle_selection_at_cursor(&mut self) {
        if let Some(row) = self.visible_rows.get(self.cursor).copied() {
            self.tree.toggle_selection(row.node_id);
            self.refresh_metrics();
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

    /// Toggles inclusion of `<git_context>` when inside a Git repository.
    pub fn toggle_git_context(&mut self) {
        if self.has_git_context {
            self.include_git_context = !self.include_git_context;
        }
    }

    /// Opens the Discarded or Unread category inspector overlay.
    pub fn open_inspector(&mut self, category: InspectorCategory) {
        self.mode = UiMode::Inspector {
            category,
            scroll: 0,
        };
    }

    /// Scrolls the active inspector modal list by `delta` rows (`isize`).
    pub fn scroll_inspector(&mut self, delta: isize) {
        if let UiMode::Inspector { category, scroll } = self.mode {
            let total = self.tree.inspector_items(category).len();
            let max_scroll = total.saturating_sub(1);
            let next = if delta.is_negative() {
                scroll.saturating_sub(delta.unsigned_abs())
            } else {
                scroll.saturating_add(delta.cast_unsigned()).min(max_scroll)
            };
            self.mode = UiMode::Inspector {
                category,
                scroll: next,
            };
        }
    }

    /// Returns from any modal dialog back to the main tree view.
    pub fn close_modal(&mut self) {
        self.mode = UiMode::Tree;
    }

    /// Initiates clipboard copy, gating on a confirmation modal if selected files carry secrets.
    pub fn request_copy(&mut self) {
        let active_secrets = self.metrics.counts.active_secret_files;
        if active_secrets > 0 {
            self.mode = UiMode::ConfirmSecrets {
                active_secret_files: active_secrets,
            };
        } else {
            self.exit_action = Some(AppExitAction::CopyToClipboard);
        }
    }

    /// Confirms copying to clipboard even though active secret warnings are present.
    pub fn confirm_copy_with_secrets(&mut self) {
        self.exit_action = Some(AppExitAction::CopyToClipboard);
    }

    /// Cancels and exits the TUI without touching the clipboard.
    pub fn cancel_exit(&mut self) {
        self.exit_action = Some(AppExitAction::Cancelled);
    }

    fn jump_to_node(&mut self, target: NodeId) {
        if let Some(pos) = self
            .visible_rows
            .iter()
            .position(|row| row.node_id == target)
        {
            self.cursor = pos;
        }
    }

    fn refresh_metrics(&mut self) {
        self.metrics = self.tree.compute_metrics();
    }

    fn refresh_visible_rows(&mut self) {
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
