//! View modes, loading progress, modal states, and exit intents for [`crate::tui::TuiAppState`].

use crate::domain::InspectorTab;
use crate::tui::state::inspector::InspectorState;

/// Active pipeline stage displayed while initializing a workspace without blocking the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingPhase {
    /// Walking filesystem directories and applying ignore/pruning filters.
    Discovering,
    /// Reading `UTF-8` files, scanning secrets, and counting BPE tokens in parallel.
    Analyzing,
    /// Restoring persisted overrides and computing initial XML overhead.
    Finalizing,
}

/// Live progress snapshot rendered during non-blocking workspace loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadingProgress {
    /// Display name of the target workspace directory.
    pub root_name: String,
    /// Current pipeline stage.
    pub phase: LoadingPhase,
    /// Directories visited so far.
    pub dirs_scanned: usize,
    /// Dependency/cache/ignored subtrees pruned so far.
    pub pruned_dirs: usize,
    /// Candidate text files queued during discovery.
    pub candidate_files: usize,
    /// Candidate files processed so far during parallel ingestion.
    pub analyzed_files: usize,
    /// Binary, invalid UTF-8, or oversized files discarded so far.
    pub discarded_files: usize,
    /// Selected tokens counted so far.
    pub selected_tokens: usize,
    /// Whether background Git metadata extraction is still running.
    pub git_pending: bool,
}

impl LoadingProgress {
    /// Creates an initial loading progress state for `root_name`.
    #[must_use]
    pub fn new(root_name: impl Into<String>) -> Self {
        Self {
            root_name: root_name.into(),
            phase: LoadingPhase::Discovering,
            dirs_scanned: 0,
            pruned_dirs: 0,
            candidate_files: 0,
            analyzed_files: 0,
            discarded_files: 0,
            selected_tokens: 0,
            git_pending: true,
        }
    }
}

/// Active view layer controlling key handling and modal overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// Primary interactive directory tree view.
    Tree,
    /// Unified tabbed inspector listing Unread, Discarded, or Secrets entries.
    Inspector {
        /// Active tab currently displayed inside the inspector.
        tab: InspectorTab,
        /// Top visible row offset inside the modal list.
        scroll: usize,
    },
    /// Quick-reference keybindings and color-legend modal overlay.
    Help,
    /// Confirmation prompt when pressing `Enter` with zero selected files.
    ConfirmEmptyCopy,
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
pub(super) enum ActiveModal {
    None,
    Inspector(InspectorState),
    Help,
    ConfirmEmptyCopy,
    ConfirmSecrets { active_secret_files: usize },
}

impl ActiveModal {
    pub(super) const fn to_ui_mode(&self) -> UiMode {
        match self {
            Self::None => UiMode::Tree,
            Self::Inspector(ins) => UiMode::Inspector {
                tab: ins.tab(),
                scroll: ins.scroll(),
            },
            Self::Help => UiMode::Help,
            Self::ConfirmEmptyCopy => UiMode::ConfirmEmptyCopy,
            Self::ConfirmSecrets {
                active_secret_files,
            } => UiMode::ConfirmSecrets {
                active_secret_files: *active_secret_files,
            },
        }
    }
}
