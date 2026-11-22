//! View modes, modal states, and exit intents for [`crate::tui::TuiAppState`].

use crate::domain::InspectorTab;
use crate::tui::state::inspector::InspectorState;

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
