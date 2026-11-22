//! Pure metric rollups and token weight tiers for [`crate::domain::DirectoryTree`].

use crate::domain::selection::FolderSelectionState;

/// Upper token bound (exclusive) for the lightweight green weight tier.
pub const LIGHT_TIER_MAX_TOKENS: usize = 2_000;
/// Upper token bound (exclusive) for the moderate yellow weight tier.
pub const MEDIUM_TIER_MAX_TOKENS: usize = 15_000;
/// Upper token bound (exclusive) for the heavy orange weight tier.
pub const HEAVY_TIER_MAX_TOKENS: usize = 50_000;

/// Visual weight classification used to color-code files and directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeightTier {
    /// Lightweight entry (`< 2,000` tokens).
    Light,
    /// Moderate entry (`2,000..15,000` tokens).
    Medium,
    /// Heavy entry (`15,000..50,000` tokens).
    Heavy,
    /// Oversized or critical entry (`>= 50,000` tokens).
    Critical,
}

impl WeightTier {
    /// Maps a token count to its corresponding visual weight tier.
    #[must_use]
    pub const fn from_tokens(tokens: usize) -> Self {
        if tokens < LIGHT_TIER_MAX_TOKENS {
            Self::Light
        } else if tokens < MEDIUM_TIER_MAX_TOKENS {
            Self::Medium
        } else if tokens < HEAVY_TIER_MAX_TOKENS {
            Self::Heavy
        } else {
            Self::Critical
        }
    }
}

/// Global counters displayed in the TUI status summary and modals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CategoryCounts {
    /// Readable text files currently selected for clipboard output.
    pub read_files: usize,
    /// Readable text files currently unselected (ignored, oversized, pruned, or user-excluded).
    pub unread_files: usize,
    /// Total discarded items (binary/corrupt/capped files plus pruned dependency dirs).
    pub discarded_items: usize,
    /// Total tokens across all selected text files.
    pub selected_tokens: usize,
    /// Number of selected files containing potential secret matches.
    pub active_secret_files: usize,
    /// Total files in the tree containing potential secret matches.
    pub total_secret_files: usize,
}

/// Aggregated metrics computed bottom-up for every node in the directory tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRollup {
    /// Tokens contributed by selected text files under this node.
    pub selected_tokens: usize,
    /// Total tokens across all readable text files under this node (selected or not).
    pub total_tokens: usize,
    /// Selected text file count inside this subtree.
    pub selected_files: usize,
    /// Unselected text file count inside this subtree.
    pub unread_files: usize,
    /// Discarded file count inside this subtree.
    pub discarded_files: usize,
    /// Derived checkbox state when the node is a directory.
    pub folder_selection: FolderSelectionState,
    /// Whether at least one selected file in this subtree carries a secret marker.
    pub has_active_secret: bool,
    /// Whether any file in this subtree carries a secret marker.
    pub has_any_secret: bool,
    /// Whether this subtree contains at least one naturally included text file.
    pub has_natural_included: bool,
}

impl Default for NodeRollup {
    fn default() -> Self {
        Self {
            selected_tokens: 0,
            total_tokens: 0,
            selected_files: 0,
            unread_files: 0,
            discarded_files: 0,
            folder_selection: FolderSelectionState::NoTextFiles,
            has_active_secret: false,
            has_any_secret: false,
            has_natural_included: false,
        }
    }
}

impl NodeRollup {
    /// Returns the token tier governing this node's visual hue.
    ///
    /// When active, hue reflects selected weight; when fully unselected, hue falls back
    /// to total subtree weight so heavy excluded entries retain a dimmed warning color.
    #[must_use]
    pub const fn display_weight_tier(&self, is_active: bool) -> WeightTier {
        let tokens = if is_active {
            self.selected_tokens
        } else {
            self.total_tokens
        };
        WeightTier::from_tokens(tokens)
    }
}

/// Full metric snapshot produced from a [`crate::domain::DirectoryTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeMetrics {
    /// Per-node rollups indexed directly by `NodeId::index()`.
    pub rollups: Vec<NodeRollup>,
    /// Global counts for the header/inspector status summaries.
    pub counts: CategoryCounts,
}
