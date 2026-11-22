//! Bounded on-demand re-scanning of previously pruned directories.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::defaults::should_prune_dir;
use crate::domain::{DirectoryTree, NodeId, PrunedDirReason, TextFileData, TextFileState};
use crate::scanner::file_reader::{ReadFileOutcome, read_bounded_utf8};
use crate::scanner::ignore_rules::IgnoreRuleStack;
use crate::scanner::walker::{ScanConfig, ScanError, read_sorted_entries};

/// Hard entry cap applied when re-scanning a pruned dependency/output directory from the TUI.
pub const DEFAULT_RESCAN_MAX_ENTRIES: usize = 2_000;

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

enum RescanStep {
    Enter {
        node_id: NodeId,
        abs_dir: PathBuf,
        rel_dir: String,
    },
    Leave,
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
    let mut tree = DirectoryTree::new(dir_name);
    let mut contents = HashMap::new();
    let mut ignore_stack = build_ancestor_ignore_stack(root, pruned_rel_path, config);

    let mut stack = vec![RescanStep::Enter {
        node_id: DirectoryTree::ROOT,
        abs_dir: target_abs,
        rel_dir: String::new(),
    }];
    let mut entries_seen = 0usize;
    let mut truncated = false;

    while let Some(step) = stack.pop() {
        match step {
            RescanStep::Leave => ignore_stack.pop_dir(),
            RescanStep::Enter {
                node_id,
                abs_dir,
                rel_dir,
            } => {
                if entries_seen >= max_entries {
                    truncated = true;
                    continue;
                }
                ignore_stack.push_dir(&abs_dir);
                stack.push(RescanStep::Leave);

                let subdirs = process_rescan_dir(
                    node_id,
                    &abs_dir,
                    &rel_dir,
                    config,
                    &ignore_stack,
                    &mut tree,
                    &mut contents,
                    &mut entries_seen,
                    max_entries,
                    &mut truncated,
                )?;
                for (sub_id, sub_abs, sub_rel) in subdirs.into_iter().rev() {
                    stack.push(RescanStep::Enter {
                        node_id: sub_id,
                        abs_dir: sub_abs,
                        rel_dir: sub_rel,
                    });
                }
            }
        }
    }

    Ok(RescanOutcome {
        tree,
        contents,
        truncated,
    })
}

#[allow(clippy::too_many_arguments)]
fn process_rescan_dir(
    parent_id: NodeId,
    abs_dir: &Path,
    rel_dir: &str,
    config: &ScanConfig,
    ignore_stack: &IgnoreRuleStack,
    tree: &mut DirectoryTree,
    contents: &mut HashMap<NodeId, String>,
    entries_seen: &mut usize,
    max_entries: usize,
    truncated: &mut bool,
) -> Result<Vec<(NodeId, PathBuf, String)>, ScanError> {
    let entries = match read_sorted_entries(abs_dir) {
        Ok(list) => list,
        Err(err) if parent_id == DirectoryTree::ROOT => return Err(err),
        Err(_) => return Ok(Vec::new()),
    };

    let mut subdirs = Vec::new();
    for entry in entries {
        if *entries_seen >= max_entries {
            *truncated = true;
            break;
        }
        *entries_seen += 1;

        let child_rel = if rel_dir.is_empty() {
            entry.name.clone()
        } else {
            format!("{rel_dir}/{}", entry.name)
        };

        if entry.is_dir {
            if should_prune_dir(&child_rel, &entry.name) {
                tree.record_pruned_dir(child_rel, PrunedDirReason::DependencyOrVcs);
                continue;
            }
            let dir_id = tree.add_dir(parent_id, &entry.name, false)?;
            subdirs.push((dir_id, entry.abs_path, child_rel));
        } else {
            let decision = ignore_stack.evaluate(&entry.abs_path, false);
            match read_bounded_utf8(&entry.abs_path, config.max_file_bytes) {
                ReadFileOutcome::Text { byte_len, content } => {
                    let mut state = TextFileState::new(decision.to_natural_selection());
                    state.mark_pruned();
                    let file_id = tree.add_text_file(
                        parent_id,
                        &entry.name,
                        TextFileData {
                            byte_len,
                            tokens: 0,
                            secret_hits: 0,
                            state,
                        },
                    )?;
                    contents.insert(file_id, content);
                }
                ReadFileOutcome::Discarded(reason) => {
                    tree.add_discarded_file(parent_id, &entry.name, reason)?;
                }
            }
        }
    }

    Ok(subdirs)
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

fn resolve_rel_path(root: &Path, rel_path: &str) -> PathBuf {
    rel_path
        .split('/')
        .filter(|seg| !seg.is_empty())
        .fold(root.to_path_buf(), |acc, seg| acc.join(seg))
}
