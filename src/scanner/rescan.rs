//! Bounded on-demand re-scanning of previously pruned directories.

use std::collections::HashMap;
use std::path::Path;

pub use crate::config::limits::DEFAULT_RESCAN_MAX_ENTRIES;
use crate::domain::{DirectoryTree, NodeId};
use crate::scanner::dir_entries::resolve_rel_path;
use crate::scanner::ignore_rules::IgnoreRuleStack;
use crate::scanner::traversal::{WalkPolicy, traverse_tree};
use crate::scanner::walker::{ScanConfig, ScanError};

/// Result produced by re-scanning a single pruned directory subtree under `max_entries`.
#[derive(Debug)]
pub struct RescanOutcome {
    /// Standalone tree rooted at the re-scanned directory name.
    pub tree: DirectoryTree,
    /// Loaded `UTF-8` payloads keyed by subtree [`NodeId`].
    pub contents: HashMap<NodeId, String>,
    /// Whether traversal stopped early after hitting `max_entries`.
    pub truncated: bool,
}

/// Re-scans `pruned_rel_path` under `root`, inheriting ancestor ignore rules up to `max_entries`.
///
/// # Errors
///
/// Returns [`ScanError`] if the target directory no longer exists or root enumeration fails.
pub fn rescan_pruned_dir(
    root: &Path,
    pruned_rel_path: &str,
    config: &ScanConfig,
    max_entries: usize,
) -> Result<RescanOutcome, ScanError> {
    let target_abs = resolve_rel_path(root, pruned_rel_path);
    if !target_abs.is_dir() {
        return Err(ScanError::NotADirectory(target_abs));
    }

    let dir_name = pruned_rel_path
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("dir");

    let out = traverse_tree(
        &target_abs,
        dir_name,
        build_ancestor_ignore_stack(root, pruned_rel_path, config),
        config,
        WalkPolicy {
            max_entries: Some(max_entries),
            mark_pruned: true,
            expand_included_dirs: false,
            expand_ignored_dirs: false,
        },
    )?;

    Ok(RescanOutcome {
        tree: out.tree,
        contents: out.contents,
        truncated: out.truncated,
    })
}

fn build_ancestor_ignore_stack(
    root: &Path,
    pruned_rel_path: &str,
    config: &ScanConfig,
) -> IgnoreRuleStack {
    let mut stack = IgnoreRuleStack::with_case_sensitivity(config.case_insensitive_ignores);
    let mut curr = root.to_path_buf();
    stack.push_dir(&curr);

    let parent_rel = pruned_rel_path.rsplit_once('/').map_or("", |(p, _)| p);
    for seg in parent_rel.split('/').filter(|s| !s.is_empty()) {
        curr = curr.join(seg);
        stack.push_dir(&curr);
    }
    stack
}
