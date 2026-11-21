//! Algebraic state types governing file classification and selection invariants.

/// Reason a readable text file is excluded by default before user overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaturalExclusion {
    /// Excluded by a `.ignore` rule at root or ancestor directory level.
    DotIgnore,
    /// Excluded by a `.gitignore` rule when not overridden by `.ignore`.
    GitIgnore,
    /// Auto-deselected because token count exceeded the configured threshold.
    ExceedsTokenThreshold,
}

/// Baseline automatic selection computed from ignore rules and token limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaturalSelection {
    /// Included by default (valid `UTF-8`, not ignored, within token threshold).
    Included,
    /// Excluded automatically for the captured reason.
    Excluded(NaturalExclusion),
}

/// Effective reason an unselected text file is omitted from `<files>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnselectedReason {
    /// Deselected manually by user action or inherited from an excluded folder.
    UserExcluded,
    /// Omitted by `.ignore`.
    DotIgnore,
    /// Omitted by `.gitignore`.
    GitIgnore,
    /// Omitted due to exceeding the default token ceiling.
    ExceedsTokenThreshold,
}

/// Reason a non-text or oversized file cannot be ingested into `<files>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDiscardReason {
    /// Binary file detected via extension, magic bytes, or null bytes.
    Binary,
    /// Byte stream contains invalid `UTF-8` sequences.
    InvalidUtf8,
    /// File size exceeds the hard safety ceiling before tokenization.
    ExceedsHardCap {
        /// Original file size in bytes.
        byte_len: u64,
    },
    /// Operating system I/O failure while reading the file.
    Unreadable,
}

/// Reason a directory subtree was pruned during scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrunedDirReason {
    /// Matches hardcoded dependency, cache, or VCS directory exclusions.
    DependencyOrVcs,
}

/// Aggregate checkbox state derived for a directory node in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSelectionState {
    /// Every readable text file inside the subtree is selected.
    AllSelected,
    /// Some readable text files are selected and some are unselected.
    Partial,
    /// No readable text file inside the subtree is selected.
    NoneSelected,
    /// Directory contains only empty subdirectories or discarded files.
    NoTextFiles,
}

/// Selection state for a readable `UTF-8` text file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFileState {
    natural: NaturalSelection,
    user_override: Option<bool>,
}

impl TextFileState {
    /// Creates a text file state initialized from its natural baseline.
    #[must_use]
    pub const fn new(natural: NaturalSelection) -> Self {
        Self {
            natural,
            user_override: None,
        }
    }

    /// Returns the baseline natural selection before user overrides.
    #[must_use]
    pub const fn natural(self) -> NaturalSelection {
        self.natural
    }

    /// Returns the explicit user override if one differs from the natural state.
    #[must_use]
    pub const fn user_override(self) -> Option<bool> {
        self.user_override
    }

    /// Returns `true` when this file will be included in the XML `<files>` section.
    #[must_use]
    pub const fn is_selected(self) -> bool {
        match self.user_override {
            Some(explicit) => explicit,
            None => matches!(self.natural, NaturalSelection::Included),
        }
    }

    /// Returns the active reason this file is excluded, or `None` if selected.
    #[must_use]
    pub const fn unselected_reason(self) -> Option<UnselectedReason> {
        if self.is_selected() {
            return None;
        }
        if matches!(self.user_override, Some(false)) {
            return Some(UnselectedReason::UserExcluded);
        }
        match self.natural {
            NaturalSelection::Included => Some(UnselectedReason::UserExcluded),
            NaturalSelection::Excluded(NaturalExclusion::DotIgnore) => {
                Some(UnselectedReason::DotIgnore)
            }
            NaturalSelection::Excluded(NaturalExclusion::GitIgnore) => {
                Some(UnselectedReason::GitIgnore)
            }
            NaturalSelection::Excluded(NaturalExclusion::ExceedsTokenThreshold) => {
                Some(UnselectedReason::ExceedsTokenThreshold)
            }
        }
    }

    /// Toggles the file explicitly and collapses overrides matching the natural baseline.
    pub fn toggle(&mut self) {
        self.set_selected(!self.is_selected());
    }

    /// Sets an explicit selection target, clearing the override if it matches natural defaults.
    pub fn set_selected(&mut self, target: bool) {
        let natural_selected = matches!(self.natural, NaturalSelection::Included);
        self.user_override = if target == natural_selected {
            None
        } else {
            Some(target)
        };
    }

    /// Restores the natural selection baseline by removing any explicit user override.
    pub fn reset_to_natural(&mut self) {
        self.user_override = None;
    }
}
