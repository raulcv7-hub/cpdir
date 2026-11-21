//! Delta override serialization and inheritance for per-directory memory persistence.

use std::collections::BTreeMap;

use crate::domain::tree::{DirectoryTree, NodeKind};

impl DirectoryTree {
    /// Applies saved per-path delta overrides top-down, preserving auto-inclusion for new files.
    pub fn apply_overrides(&mut self, saved: &BTreeMap<String, bool>) {
        let len = self.nodes.len();
        let mut inherited_dir_override: Vec<Option<bool>> = vec![None; len];

        for idx in 0..len {
            let parent_inherited = self.nodes[idx]
                .parent
                .and_then(|pid| inherited_dir_override[pid.0]);
            let rel_path = self.nodes[idx].rel_path.clone();
            let explicit = saved.get(&rel_path).copied();

            match &mut self.nodes[idx].kind {
                NodeKind::Directory(dir) => {
                    let effective = if idx == 0 {
                        None
                    } else {
                        explicit.or(parent_inherited)
                    };
                    dir.folder_override = effective;
                    inherited_dir_override[idx] = effective;
                }
                NodeKind::TextFile(file) => {
                    if let Some(val) = explicit {
                        file.state.set_selected(val);
                    } else if let Some(false) = parent_inherited {
                        file.state.set_selected(false);
                    }
                }
                NodeKind::DiscardedFile(_) => {}
            }
        }
    }

    /// Extracts minimal delta overrides suitable for JSON persistence across runs.
    #[must_use]
    pub fn collect_overrides(&self) -> BTreeMap<String, bool> {
        let len = self.nodes.len();
        let mut inherited_excluded = vec![false; len];
        let mut out = BTreeMap::new();

        for (idx, node) in self.nodes.iter().enumerate() {
            let parent_excluded = node.parent.is_some_and(|pid| inherited_excluded[pid.0]);

            match &node.kind {
                NodeKind::Directory(dir) => {
                    if idx != 0 && dir.folder_override == Some(false) {
                        if !parent_excluded {
                            out.insert(node.rel_path.clone(), false);
                        }
                        inherited_excluded[idx] = true;
                    }
                }
                NodeKind::TextFile(file) => {
                    if parent_excluded {
                        if file.state.is_selected() {
                            out.insert(node.rel_path.clone(), true);
                        }
                    } else if let Some(val) = file.state.user_override() {
                        out.insert(node.rel_path.clone(), val);
                    }
                }
                NodeKind::DiscardedFile(_) => {}
            }
        }

        out
    }
}
