//! Core TUI state coordinating directory metrics, navigation, context accounting, and modals.

use crate::domain::{DirectoryTree, TreeMetrics, VisibleRow};
use crate::tui::state::context_tokens::ContextTokenState;
use crate::tui::state::inspector::InspectorState;
use crate::tui::state::modes::{ActiveModal, AppExitAction, UiMode};
use crate::tui::state::tree_nav::TreeNavigator;

/// Pure UI state holding the directory tree, cached metrics, tree navigator, and modal state.
#[derive(Debug, Clone)]
pub struct TuiAppState {
    pub(super) tree: DirectoryTree,
    metrics: TreeMetrics,
    pub(super) nav: TreeNavigator,
    context: ContextTokenState,
    pub(super) pending_reingest: Option<String>,
    pub(super) modal: ActiveModal,
    exit_action: Option<AppExitAction>,
    selection_revision: u64,
}

impl TuiAppState {
    /// Initializes TUI state from a scanned and tokenized [`DirectoryTree`].
    #[must_use]
    pub fn new(tree: DirectoryTree, has_git_context: bool) -> Self {
        let metrics = tree.compute_metrics();
        let nav = TreeNavigator::new(&tree);
        Self {
            tree,
            metrics,
            nav,
            context: ContextTokenState::new(has_git_context),
            pending_reingest: None,
            modal: ActiveModal::None,
            exit_action: None,
            selection_revision: 0,
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

    /// Returns an immutable reference to the tree cursor/scroll navigator.
    #[must_use]
    pub const fn nav(&self) -> &TreeNavigator {
        &self.nav
    }

    /// Returns a mutable reference to the tree cursor/scroll navigator.
    pub const fn nav_mut(&mut self) -> &mut TreeNavigator {
        &mut self.nav
    }

    /// Returns flattened visible rows matching currently expanded directories.
    #[must_use]
    pub fn visible_rows(&self) -> &[VisibleRow] {
        self.nav.visible_rows()
    }

    /// Returns the zero-based cursor row inside [`Self::visible_rows`].
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.nav.cursor()
    }

    /// Returns the top visible row index of the tree viewport.
    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.nav.scroll_offset()
    }

    /// Returns the active [`InspectorState`] if the inspector modal is open.
    #[must_use]
    pub const fn inspector(&self) -> Option<&InspectorState> {
        match &self.modal {
            ActiveModal::Inspector(ins) => Some(ins),
            _ => None,
        }
    }

    /// Returns `true` when the workspace is inside a valid Git repository.
    #[must_use]
    pub const fn has_git_context(&self) -> bool {
        self.context.has_git_context()
    }

    /// Returns `true` while background Git metadata extraction is still in flight.
    #[must_use]
    pub const fn is_git_pending(&self) -> bool {
        self.context.is_git_pending()
    }

    /// Marks background Git extraction pending or finished.
    pub const fn set_git_pending(&mut self, pending: bool) {
        self.context.set_git_pending(pending);
    }

    /// Attaches resolved Git metadata tokens and initial toggle state from background loading.
    pub fn attach_git_context(&mut self, tokens: usize, saved_enabled: Option<bool>) {
        self.context.attach_git_context(tokens, saved_enabled);
    }

    /// Returns `true` when `<git_context>` will be included in the copied XML.
    #[must_use]
    pub const fn include_git_context(&self) -> bool {
        self.context.include_git_context()
    }

    /// Stores the `o200k_base` token count of `<git_context>` for live header updates.
    pub const fn set_git_context_tokens(&mut self, tokens: usize) {
        self.context.set_git_context_tokens(tokens);
    }

    /// Returns the `o200k_base` token count contributed by `<git_context>` when enabled.
    #[must_use]
    pub const fn git_context_tokens(&self) -> usize {
        self.context.git_context_tokens()
    }

    /// Stores XML summary, directory structure, envelope, and file-tag token overhead.
    pub const fn set_xml_overhead_tokens(&mut self, tokens: usize) {
        self.context.set_xml_overhead_tokens(tokens);
    }

    /// Returns the non-Git XML metadata and wrapper token overhead included in the live total.
    #[must_use]
    pub const fn xml_overhead_tokens(&self) -> usize {
        self.context.xml_overhead_tokens()
    }

    /// Returns total selected file tokens plus active XML and `<git_context>` tokens.
    #[must_use]
    pub const fn total_effective_tokens(&self) -> usize {
        self.context
            .total_effective_tokens(self.metrics.counts.selected_tokens)
    }

    /// Returns a collision-free monotonic revision identifying when XML overhead needs refresh.
    #[must_use]
    pub const fn overhead_fingerprint(&self) -> (u64, bool) {
        (self.selection_revision, self.include_abs_path())
    }

    /// Overrides the initial Git toggle state (e.g., from persisted directory settings).
    pub const fn set_include_git_context(&mut self, enabled: bool) {
        self.context.set_include_git_context(enabled);
    }

    /// Toggles inclusion of `<git_context>` when inside a Git repository.
    pub const fn toggle_git_context(&mut self) {
        self.context.toggle_git_context();
    }

    /// Returns `true` when normalized `root_path` will be emitted inside `<context_summary>`.
    #[must_use]
    pub const fn include_abs_path(&self) -> bool {
        self.context.include_abs_path()
    }

    /// Sets opt-in `root_path` emission state (defaults to `false` for privacy).
    pub const fn set_include_abs_path(&mut self, enabled: bool) {
        self.context.set_include_abs_path(enabled);
    }

    /// Toggles opt-in `root_path` emission inside `<context_summary>`.
    pub const fn toggle_abs_path(&mut self) {
        self.context.toggle_abs_path();
    }

    /// Returns the current UI mode (main tree or active modal).
    #[must_use]
    pub const fn mode(&self) -> UiMode {
        self.modal.to_ui_mode()
    }

    /// Returns the exit decision once the user confirms or cancels.
    #[must_use]
    pub const fn exit_action(&self) -> Option<AppExitAction> {
        self.exit_action
    }

    /// Returns `true` if an on-demand pruned directory re-ingest has been requested.
    #[must_use]
    pub const fn has_pending_reingest(&self) -> bool {
        self.pending_reingest.is_some()
    }

    /// Toggles the `?` quick-reference help modal overlay.
    pub fn toggle_help(&mut self) {
        self.modal = match self.modal {
            ActiveModal::Help => ActiveModal::None,
            _ => ActiveModal::Help,
        };
    }

    /// Returns from any modal dialog back to the main tree view.
    pub fn close_modal(&mut self) {
        self.modal = ActiveModal::None;
    }

    /// Initiates clipboard copy, gating on empty selection or selected secret confirmation.
    pub fn request_copy(&mut self) {
        let counts = self.metrics.counts;
        if counts.read_files == 0 {
            self.modal = ActiveModal::ConfirmEmptyCopy;
        } else if counts.active_secret_files > 0 {
            self.modal = ActiveModal::ConfirmSecrets {
                active_secret_files: counts.active_secret_files,
            };
        } else {
            self.exit_action = Some(AppExitAction::CopyToClipboard);
        }
    }

    /// Confirms copying to clipboard from any confirmation modal.
    pub const fn confirm_copy_anyway(&mut self) {
        self.exit_action = Some(AppExitAction::CopyToClipboard);
    }

    /// Cancels and exits the TUI without touching the clipboard.
    pub const fn cancel_exit(&mut self) {
        self.exit_action = Some(AppExitAction::Cancelled);
    }

    pub(super) fn refresh_metrics(&mut self) {
        self.metrics = self.tree.compute_metrics();
        self.selection_revision = self.selection_revision.saturating_add(1);
    }
}
