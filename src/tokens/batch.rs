//! Adapter applying [`TokenCounter`] counts and threshold deselection to [`DirectoryTree`].

use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::domain::{DirectoryTree, NodeId, NodeKind};
use crate::tokens::counter::TokenCounter;

/// Counts tokens for all readable text files in `tree` and auto-deselects entries above `threshold`.
pub fn populate_token_counts<S: BuildHasher>(
    tree: &mut DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
    auto_deselect_threshold: usize,
    counter: TokenCounter,
) {
    let work: Vec<(NodeId, &str)> = tree
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if matches!(node.kind, NodeKind::TextFile(_)) {
                let id = NodeId(idx);
                contents.get(&id).map(|text| (id, text.as_str()))
            } else {
                None
            }
        })
        .collect();

    for (id, token_count) in counter.count_batch(&work) {
        if let Some(node) = tree.get_mut(id)
            && let NodeKind::TextFile(file) = &mut node.kind
        {
            file.tokens = token_count;
            if token_count > auto_deselect_threshold {
                file.state.mark_exceeds_token_threshold();
            }
        }
    }
}
