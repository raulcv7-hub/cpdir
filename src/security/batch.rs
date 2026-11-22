//! Adapter applying [`SecretScanner`] findings onto a [`DirectoryTree`].

use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::domain::{DirectoryTree, NodeId, NodeKind};
use crate::security::scanner::SecretScanner;

/// Scans all readable text files in `tree` and stores finding counts and details per file.
pub fn populate_secret_hits<S: BuildHasher>(
    tree: &mut DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
    scanner: &SecretScanner,
) {
    let work: Vec<(NodeId, &str, &str)> = tree
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if matches!(node.kind, NodeKind::TextFile(_)) {
                let id = NodeId(idx);
                contents
                    .get(&id)
                    .map(|text| (id, node.rel_path.as_str(), text.as_str()))
            } else {
                None
            }
        })
        .collect();

    for (id, findings) in scanner.scan_batch(&work) {
        tree.set_secret_findings(id, findings);
    }
}
