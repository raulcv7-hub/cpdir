//! Shared iterative directory traversal engine powering full scans and bounded re-scans.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::rules::should_prune_dir;
use crate::domain::{DirectoryTree, NodeId, PrunedDirReason, TextFileData, TextFileState};
use crate::scanner::file_reader::{ReadFileOutcome, read_bounded_utf8};
use crate::scanner::ignore_rules::{IgnoreDecision, IgnoreRuleStack};
use crate::scanner::walker::{ScanConfig, ScanError};

pub(super) struct WalkPolicy {
    pub(super) max_entries: Option<usize>,
    pub(super) mark_pruned: bool,
    pub(super) expand_included_dirs: bool,
    pub(super) expand_ignored_dirs: bool,
}

pub(super) struct TraversalOutput {
    pub(super) tree: DirectoryTree,
    pub(super) contents: HashMap<NodeId, String>,
    pub(super) truncated: bool,
}

enum WalkStep {
    Enter {
        node_id: NodeId,
        abs_dir: PathBuf,
        rel_dir: String,
    },
    Leave,
}

struct PendingSubdir {
    node_id: NodeId,
    abs_dir: PathBuf,
    rel_dir: String,
}

struct SortedEntry {
    name: String,
    lower_name: String,
    abs_path: PathBuf,
    is_dir: bool,
}

struct WalkCtx<'a> {
    config: &'a ScanConfig,
    policy: WalkPolicy,
    ignore_stack: IgnoreRuleStack,
    tree: DirectoryTree,
    contents: HashMap<NodeId, String>,
    entries_seen: usize,
    truncated: bool,
}

pub(super) fn traverse_tree(
    root_abs: &Path,
    root_name: &str,
    ignore_stack: IgnoreRuleStack,
    config: &ScanConfig,
    policy: WalkPolicy,
) -> Result<TraversalOutput, ScanError> {
    let mut ctx = WalkCtx {
        config,
        policy,
        ignore_stack,
        tree: DirectoryTree::new(root_name),
        contents: HashMap::new(),
        entries_seen: 0,
        truncated: false,
    };

    let mut stack = vec![WalkStep::Enter {
        node_id: DirectoryTree::ROOT,
        abs_dir: root_abs.to_path_buf(),
        rel_dir: String::new(),
    }];

    while let Some(step) = stack.pop() {
        match step {
            WalkStep::Leave => ctx.ignore_stack.pop_dir(),
            WalkStep::Enter {
                node_id,
                abs_dir,
                rel_dir,
            } => {
                if ctx.reached_entry_cap() {
                    ctx.truncated = true;
                    continue;
                }
                ctx.ignore_stack.push_dir(&abs_dir);
                stack.push(WalkStep::Leave);

                let subdirs = ctx.process_directory(node_id, &abs_dir, &rel_dir)?;
                for sub in subdirs.into_iter().rev() {
                    stack.push(WalkStep::Enter {
                        node_id: sub.node_id,
                        abs_dir: sub.abs_dir,
                        rel_dir: sub.rel_dir,
                    });
                }
            }
        }
    }

    Ok(TraversalOutput {
        tree: ctx.tree,
        contents: ctx.contents,
        truncated: ctx.truncated,
    })
}

pub(crate) fn resolve_rel_path(root: &Path, rel_path: &str) -> PathBuf {
    rel_path
        .split('/')
        .filter(|seg| !seg.is_empty())
        .fold(root.to_path_buf(), |acc, seg| acc.join(seg))
}

impl WalkCtx<'_> {
    fn reached_entry_cap(&self) -> bool {
        self.policy
            .max_entries
            .is_some_and(|cap| self.entries_seen >= cap)
    }

    fn process_directory(
        &mut self,
        parent_id: NodeId,
        abs_dir: &Path,
        rel_dir: &str,
    ) -> Result<Vec<PendingSubdir>, ScanError> {
        let entries = match read_sorted_entries(abs_dir) {
            Ok(list) => list,
            Err(err) if parent_id == DirectoryTree::ROOT => return Err(err),
            // Skip protected or transient subdirectories without aborting the rest of the tree.
            Err(_) => return Ok(Vec::new()),
        };

        let mut subdirs = Vec::new();
        for entry in entries {
            if self.reached_entry_cap() {
                self.truncated = true;
                break;
            }
            self.entries_seen += 1;

            let child_rel = if rel_dir.is_empty() {
                entry.name.clone()
            } else {
                format!("{rel_dir}/{}", entry.name)
            };

            if entry.is_dir {
                if should_prune_dir(&child_rel, &entry.name) {
                    self.tree
                        .record_pruned_dir(child_rel, PrunedDirReason::DependencyOrVcs);
                    continue;
                }
                let is_expanded = self.policy.expand_ignored_dirs
                    || (self.policy.expand_included_dirs
                        && self.ignore_stack.evaluate(&entry.abs_path, true)
                            == IgnoreDecision::Included);
                let dir_id = self.tree.add_dir(parent_id, &entry.name, is_expanded)?;
                subdirs.push(PendingSubdir {
                    node_id: dir_id,
                    abs_dir: entry.abs_path,
                    rel_dir: child_rel,
                });
            } else {
                self.ingest_file(parent_id, &entry)?;
            }
        }

        Ok(subdirs)
    }

    fn ingest_file(&mut self, parent_id: NodeId, entry: &SortedEntry) -> Result<(), ScanError> {
        let decision = self.ignore_stack.evaluate(&entry.abs_path, false);
        match read_bounded_utf8(&entry.abs_path, self.config.max_file_bytes) {
            ReadFileOutcome::Text { byte_len, content } => {
                let mut state = TextFileState::new(decision.to_natural_selection());
                if self.policy.mark_pruned {
                    state.mark_pruned();
                }
                let file_id = self.tree.add_text_file(
                    parent_id,
                    &entry.name,
                    TextFileData {
                        byte_len,
                        tokens: 0,
                        secret_hits: 0,
                        state,
                    },
                )?;
                self.contents.insert(file_id, content);
            }
            ReadFileOutcome::Discarded(reason) => {
                self.tree
                    .add_discarded_file(parent_id, &entry.name, reason)?;
            }
        }
        Ok(())
    }
}

fn read_sorted_entries(dir: &Path) -> Result<Vec<SortedEntry>, ScanError> {
    let read_dir = fs::read_dir(dir).map_err(|err| ScanError::ReadDirFailed {
        path: dir.to_path_buf(),
        reason: err.to_string(),
    })?;

    let mut entries = Vec::new();
    for item_res in read_dir {
        let Ok(item) = item_res else {
            continue;
        };
        // Never follow NTFS junctions or symlinks to prevent directory loops and escapes.
        let Ok(ft) = item.file_type() else {
            continue;
        };
        if ft.is_symlink() || (!ft.is_dir() && !ft.is_file()) {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if name.is_empty() {
            continue;
        }
        entries.push(SortedEntry {
            lower_name: name.to_ascii_lowercase(),
            name,
            abs_path: item.path(),
            is_dir: ft.is_dir(),
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a
            .lower_name
            .cmp(&b.lower_name)
            .then_with(|| a.name.cmp(&b.name)),
    });

    Ok(entries)
}
