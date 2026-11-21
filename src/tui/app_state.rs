//! Pure TUI state machine driving selection, modals, and exit intents.

use crate::domain::{DirectoryTree, InspectorCategory, InspectorItem, TreeMetrics, VisibleRow};
use crate::tui::inspector_state::InspectorState;

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

#[derive(Debug, Clone)]
enum ActiveModal {
    None,
    Inspector(InspectorState),
    ConfirmSecrets { active_secret_files: usize },
}

/// Pure UI state holding the directory tree, cached metrics, cursor, and modal modes.
#[derive(Debug, Clone)]
pub struct TuiAppState {
    pub(super) tree: DirectoryTree,
    metrics: TreeMetrics,
    pub(super) visible_rows: Vec<VisibleRow>,
    pub(super) cursor: usize,
    pub(super) scroll_offset: usize,
    has_git_context: bool,
    include_git_context: bool,
    modal: ActiveModal,
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
            modal: ActiveModal::None,
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
        match &self.modal {
            ActiveModal::None => UiMode::Tree,
            ActiveModal::Inspector(ins) => UiMode::Inspector {
                category: ins.category(),
                scroll: ins.scroll(),
            },
            ActiveModal::ConfirmSecrets {
                active_secret_files,
            } => UiMode::ConfirmSecrets {
                active_secret_files: *active_secret_files,
            },
        }
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

    pub(crate) const fn active_inspector(&self) -> Option<&InspectorState> {
        match &self.modal {
            ActiveModal::Inspector(ins) => Some(ins),
            ActiveModal::None | ActiveModal::ConfirmSecrets { .. } => None,
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
        self.modal = ActiveModal::Inspector(InspectorState::new(&self.tree, category));
    }

    /// Scrolls the active inspector modal list by `delta` rows (`isize`).
    pub fn scroll_inspector(&mut self, delta: isize) {
        if let ActiveModal::Inspector(ins) = &mut self.modal {
            ins.scroll_by(delta);
        }
    }

    /// Returns from any modal dialog back to the main tree view.
    pub fn close_modal(&mut self) {
        self.modal = ActiveModal::None;
    }

    /// Initiates clipboard copy, gating on a confirmation modal if selected files carry secrets.
    pub fn request_copy(&mut self) {
        let active_secrets = self.metrics.counts.active_secret_files;
        if active_secrets > 0 {
            self.modal = ActiveModal::ConfirmSecrets {
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

    fn refresh_metrics(&mut self) {
        self.metrics = self.tree.compute_metrics();
    }
}
