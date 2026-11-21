//! Cached state for the Discarded/Unread category inspector modal.

use crate::domain::{DirectoryTree, InspectorCategory, InspectorItem};

/// State backing the Discarded/Unread modal list without recomputing items per frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorState {
    category: InspectorCategory,
    items: Vec<InspectorItem>,
    scroll: usize,
}

impl InspectorState {
    /// Opens an inspector snapshot for `category` populated from `tree`.
    #[must_use]
    pub(crate) fn new(tree: &DirectoryTree, category: InspectorCategory) -> Self {
        Self {
            category,
            items: tree.inspector_items(category),
            scroll: 0,
        }
    }

    /// Returns the category currently displayed inside the modal.
    #[must_use]
    pub(crate) const fn category(&self) -> InspectorCategory {
        self.category
    }

    /// Returns the top visible row offset.
    #[must_use]
    pub(crate) const fn scroll(&self) -> usize {
        self.scroll
    }

    /// Returns the cached rows rendered inside the modal.
    #[must_use]
    pub(crate) fn items(&self) -> &[InspectorItem] {
        &self.items
    }

    /// Adjusts the top scroll offset by `delta` rows clamped to valid item bounds.
    pub(crate) fn scroll_by(&mut self, delta: isize) {
        let max_scroll = self.items.len().saturating_sub(1);
        self.scroll = if delta.is_negative() {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll
                .saturating_add(delta.cast_unsigned())
                .min(max_scroll)
        };
    }
}
