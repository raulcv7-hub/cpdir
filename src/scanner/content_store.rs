//! Bounded memory lifecycle for large unselected file buffers and on-demand XML hydration.

use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasher;
use std::path::Path;

pub use crate::config::limits::INLINE_CONTENT_CACHE_BYTES;
use crate::domain::{DirectoryTree, FileDiscardReason, NodeId, NodeKind};
use crate::scanner::file_reader::{ReadFileOutcome, read_bounded_utf8};
use crate::scanner::traversal::resolve_rel_path;

/// Errors emitted when re-reading an unselected file that the user enabled prior to copying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentHydrationError {
    /// Target file vanished, grew past the byte cap, or ceased to be valid `UTF-8` before copy.
    FileUnavailable {
        /// Forward-slash relative path inside the scanned workspace.
        rel_path: String,
        /// Underlying reader classification explaining why hydration failed.
        reason: FileDiscardReason,
    },
}

impl fmt::Display for ContentHydrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileUnavailable { rel_path, reason } => {
                let detail = match reason {
                    FileDiscardReason::Binary => "became a binary file".to_owned(),
                    FileDiscardReason::InvalidUtf8 => "is no longer valid UTF-8".to_owned(),
                    FileDiscardReason::ExceedsHardCap { byte_len } => {
                        format!("grew to {byte_len} bytes above the 5 MiB cap")
                    }
                    FileDiscardReason::Unreadable => "is no longer readable on disk".to_owned(),
                };
                write!(f, "cannot include {rel_path}: file {detail}")
            }
        }
    }
}

impl std::error::Error for ContentHydrationError {}

/// Drops cached `UTF-8` payloads for unselected files larger than `max_inline_bytes`.
///
/// Returns the number of released buffers so large ignored dumps and `>50k` files do not pin RAM
/// while the interactive TUI is open.
pub fn release_large_unselected_contents<S: BuildHasher>(
    tree: &DirectoryTree,
    contents: &mut HashMap<NodeId, String, S>,
    max_inline_bytes: u64,
) -> usize {
    let mut released = 0usize;
    for (idx, node) in tree.nodes().iter().enumerate() {
        if let NodeKind::TextFile(file) = &node.kind
            && !file.state.is_selected()
            && file.byte_len > max_inline_bytes
        {
            let id = NodeId(idx);
            if contents.remove(&id).is_some() {
                released += 1;
            }
        }
    }
    if released > 0 {
        contents.shrink_to_fit();
    }
    released
}

/// Reloads from disk any selected text file whose buffer was previously released from RAM.
///
/// # Errors
///
/// Returns [`ContentHydrationError::FileUnavailable`] if a newly selected file was deleted, became
/// binary/corrupt `UTF-8`, or grew past `max_file_bytes` before copy.
pub fn hydrate_selected_contents<S: BuildHasher>(
    root: &Path,
    tree: &DirectoryTree,
    contents: &mut HashMap<NodeId, String, S>,
    max_file_bytes: u64,
) -> Result<(), ContentHydrationError> {
    let pending: Vec<(NodeId, String)> = tree
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| match &node.kind {
            NodeKind::TextFile(file) if file.state.is_selected() => {
                let id = NodeId(idx);
                (!contents.contains_key(&id)).then(|| (id, node.rel_path.clone()))
            }
            _ => None,
        })
        .collect();

    for (id, rel_path) in pending {
        let disk_path = resolve_rel_path(root, &rel_path);
        match read_bounded_utf8(&disk_path, max_file_bytes) {
            ReadFileOutcome::Text { content, .. } => {
                contents.insert(id, content);
            }
            ReadFileOutcome::Discarded(reason) => {
                return Err(ContentHydrationError::FileUnavailable { rel_path, reason });
            }
        }
    }
    Ok(())
}
