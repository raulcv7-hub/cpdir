//! Iterative `O(N)` bottom-up metric and secret aggregation across [`DirectoryTree`].

use crate::domain::metrics::{CategoryCounts, NodeRollup, TreeMetrics};
use crate::domain::selection::{FolderSelectionState, NaturalSelection};
use crate::domain::tree::{DirectoryTree, NodeKind};

impl DirectoryTree {
    /// Computes bottom-up node rollups and global counters in linear `O(N)` time.
    ///
    /// Relies on the arena invariant that every parent index precedes its children,
    /// avoiding recursion and stack overflow risk on deep Windows directory trees.
    #[must_use]
    pub fn compute_metrics(&self) -> TreeMetrics {
        let len = self.nodes.len();
        let mut rollups = vec![NodeRollup::default(); len];
        let mut counts = CategoryCounts {
            discarded_items: self.pruned_dirs.len(),
            ..CategoryCounts::default()
        };

        for (idx, node) in self.nodes.iter().enumerate().rev() {
            match &node.kind {
                NodeKind::TextFile(file) => {
                    let selected = file.state.is_selected();
                    let has_secret = file.secret_hits > 0;
                    let is_natural = matches!(file.state.natural(), NaturalSelection::Included);

                    rollups[idx] = NodeRollup {
                        selected_tokens: if selected { file.tokens } else { 0 },
                        total_tokens: file.tokens,
                        selected_files: usize::from(selected),
                        unread_files: usize::from(!selected),
                        discarded_files: 0,
                        folder_selection: FolderSelectionState::NoTextFiles,
                        has_active_secret: selected && has_secret,
                        has_any_secret: has_secret,
                        has_natural_included: is_natural,
                    };

                    if selected {
                        counts.read_files += 1;
                        counts.selected_tokens += file.tokens;
                        if has_secret {
                            counts.active_secret_files += 1;
                        }
                    } else {
                        counts.unread_files += 1;
                    }
                    if has_secret {
                        counts.total_secret_files += 1;
                    }
                }
                NodeKind::DiscardedFile(_) => {
                    rollups[idx].discarded_files = 1;
                    counts.discarded_items += 1;
                }
                NodeKind::Directory(dir) => {
                    let mut agg = NodeRollup::default();
                    for &child in &dir.children {
                        let child_r = rollups[child.index()];
                        agg.selected_tokens += child_r.selected_tokens;
                        agg.total_tokens += child_r.total_tokens;
                        agg.selected_files += child_r.selected_files;
                        agg.unread_files += child_r.unread_files;
                        agg.discarded_files += child_r.discarded_files;
                        agg.has_active_secret |= child_r.has_active_secret;
                        agg.has_any_secret |= child_r.has_any_secret;
                        agg.has_natural_included |= child_r.has_natural_included;
                    }
                    agg.folder_selection =
                        derive_folder_selection(agg.selected_files, agg.unread_files);
                    rollups[idx] = agg;
                }
            }
        }

        TreeMetrics { rollups, counts }
    }
}

const fn derive_folder_selection(selected: usize, unread: usize) -> FolderSelectionState {
    match (selected, unread) {
        (0, 0) => FolderSelectionState::NoTextFiles,
        (_, 0) => FolderSelectionState::AllSelected,
        (0, _) => FolderSelectionState::NoneSelected,
        (_, _) => FolderSelectionState::Partial,
    }
}
