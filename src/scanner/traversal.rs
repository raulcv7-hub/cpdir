//! Shared iterative directory discovery engine decoupling tree walking from file buffering.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::rules::{is_known_binary_extension, should_prune_dir};
use crate::domain::{
    DirectoryTree, FileDiscardReason, NaturalSelection, NodeId, PrunedDirReason, TextFileData,
    TextFileState,
};
use crate::scanner::dir_entries::{SortedEntry, read_sorted_entries};
use crate::scanner::file_reader::is_cloud_placeholder;
use crate::scanner::ignore_rules::{IgnoreDecision, IgnoreRuleStack};
use crate::scanner::walker::{ScanConfig, ScanError};

/// Live discovery counters emitted while scanning filesystem directories.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DiscoveryProgress {
    pub(crate) dirs_scanned: usize,
    pub(crate) pruned_dirs: usize,
    pub(crate) candidate_files: usize,
    pub(crate) discarded_files: usize,
}

pub(crate) struct DiscoveredFile {
    pub(crate) node_id: NodeId,
    pub(crate) abs_path: PathBuf,
    pub(crate) rel_path: String,
    pub(crate) natural: NaturalSelection,
    pub(crate) mark_pruned: bool,
}

pub(crate) struct DiscoveryOutput {
    pub(crate) tree: DirectoryTree,
    pub(crate) files: Vec<DiscoveredFile>,
    pub(crate) truncated: bool,
    pub(crate) cancelled: bool,
}

pub(crate) struct WalkPolicy<'a> {
    pub(crate) max_entries: Option<usize>,
    pub(crate) ignored_dir_prune_threshold: Option<usize>,
    pub(crate) rel_prefix: &'a str,
    pub(crate) mark_pruned: bool,
    pub(crate) expand_included_dirs: bool,
    pub(crate) expand_ignored_dirs: bool,
}

pub(crate) struct WalkRequest<'a> {
    pub(crate) root_abs: &'a Path,
    pub(crate) root_name: &'a str,
    pub(crate) ignore_stack: IgnoreRuleStack,
    pub(crate) config: &'a ScanConfig,
    pub(crate) policy: WalkPolicy<'a>,
    pub(crate) cancel: Option<&'a AtomicBool>,
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

struct WalkCtx<'a> {
    config: &'a ScanConfig,
    policy: WalkPolicy<'a>,
    ignore_stack: IgnoreRuleStack,
    tree: DirectoryTree,
    files: Vec<DiscoveredFile>,
    entries_seen: usize,
    dirs_scanned: usize,
    pruned_dirs: usize,
    discarded_files: usize,
    truncated: bool,
    cancelled: bool,
    cancel: Option<&'a AtomicBool>,
}

pub(crate) fn discover_tree(
    req: WalkRequest<'_>,
    on_progress: &mut dyn FnMut(DiscoveryProgress),
) -> Result<DiscoveryOutput, ScanError> {
    let mut ctx = WalkCtx {
        config: req.config,
        policy: req.policy,
        ignore_stack: req.ignore_stack,
        tree: DirectoryTree::new(req.root_name),
        files: Vec::new(),
        entries_seen: 0,
        dirs_scanned: 0,
        pruned_dirs: 0,
        discarded_files: 0,
        truncated: false,
        cancelled: false,
        cancel: req.cancel,
    };

    let mut stack = vec![WalkStep::Enter {
        node_id: DirectoryTree::ROOT,
        abs_dir: req.root_abs.to_path_buf(),
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
                if ctx.is_cancelled() {
                    ctx.cancelled = true;
                    break;
                }
                if ctx.reached_entry_cap() {
                    ctx.truncated = true;
                    continue;
                }
                ctx.ignore_stack.push_dir(&abs_dir);
                stack.push(WalkStep::Leave);

                ctx.dirs_scanned += 1;
                let subdirs = ctx.process_directory(node_id, &abs_dir, &rel_dir)?;
                on_progress(DiscoveryProgress {
                    dirs_scanned: ctx.dirs_scanned,
                    pruned_dirs: ctx.pruned_dirs,
                    candidate_files: ctx.files.len(),
                    discarded_files: ctx.discarded_files,
                });
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

    Ok(DiscoveryOutput {
        tree: ctx.tree,
        files: ctx.files,
        truncated: ctx.truncated,
        cancelled: ctx.cancelled,
    })
}

impl WalkCtx<'_> {
    fn is_cancelled(&self) -> bool {
        self.cancel.is_some_and(|flag| flag.load(Ordering::Acquire))
    }

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
            if self.is_cancelled() {
                self.cancelled = true;
                break;
            }
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
                if let Some(sub) = self.visit_subdir(parent_id, entry, child_rel)? {
                    subdirs.push(sub);
                }
            } else {
                self.queue_file(parent_id, entry, child_rel)?;
            }
        }

        Ok(subdirs)
    }

    fn visit_subdir(
        &mut self,
        parent_id: NodeId,
        entry: SortedEntry,
        child_rel: String,
    ) -> Result<Option<PendingSubdir>, ScanError> {
        let workspace_rel = if self.policy.rel_prefix.is_empty() {
            child_rel.clone()
        } else {
            format!("{}/{child_rel}", self.policy.rel_prefix)
        };
        if should_prune_dir(&workspace_rel, &entry.name) {
            self.tree
                .record_pruned_dir(child_rel, PrunedDirReason::DependencyOrVcs);
            self.pruned_dirs += 1;
            return Ok(None);
        }

        let dir_decision = self.ignore_stack.evaluate(&entry.abs_path, true);
        if matches!(
            dir_decision,
            IgnoreDecision::ExcludedByGitIgnore | IgnoreDecision::ExcludedByDotIgnore
        ) && !self.ignore_stack.has_dot_ignore_whitelist()
            && self.policy.ignored_dir_prune_threshold.is_some_and(|cap| {
                should_prune_large_ignored_dir(&entry.abs_path, cap, self.cancel)
            })
        {
            self.tree
                .record_pruned_dir(child_rel, PrunedDirReason::IgnoredSubtree);
            self.pruned_dirs += 1;
            return Ok(None);
        }

        let is_expanded = self.policy.expand_ignored_dirs
            || (self.policy.expand_included_dirs && dir_decision == IgnoreDecision::Included);
        let dir_id = self.tree.add_dir(parent_id, &entry.name, is_expanded)?;
        Ok(Some(PendingSubdir {
            node_id: dir_id,
            abs_dir: entry.abs_path,
            rel_dir: child_rel,
        }))
    }

    fn queue_file(
        &mut self,
        parent_id: NodeId,
        entry: SortedEntry,
        child_rel: String,
    ) -> Result<(), ScanError> {
        if let Some(reason) = precheck_file_discard(&entry.abs_path, self.config.max_file_bytes) {
            self.tree
                .add_discarded_file(parent_id, &entry.name, reason)?;
            self.discarded_files += 1;
            return Ok(());
        }

        let natural = self
            .ignore_stack
            .evaluate(&entry.abs_path, false)
            .to_natural_selection();
        let mut state = TextFileState::new(natural);
        if self.policy.mark_pruned {
            state.mark_pruned();
        }
        let file_id = self.tree.add_text_file(
            parent_id,
            &entry.name,
            TextFileData {
                byte_len: 0,
                tokens: 0,
                secret_hits: 0,
                state,
            },
        )?;
        self.files.push(DiscoveredFile {
            node_id: file_id,
            abs_path: entry.abs_path,
            rel_path: child_rel,
            natural,
            mark_pruned: self.policy.mark_pruned,
        });
        Ok(())
    }
}

fn precheck_file_discard(path: &Path, max_file_bytes: u64) -> Option<FileDiscardReason> {
    if is_known_binary_extension(path) {
        return Some(FileDiscardReason::Binary);
    }
    if is_cloud_placeholder(path) {
        return Some(FileDiscardReason::Unreadable);
    }
    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.is_file()
        && meta.len() > max_file_bytes
    {
        return Some(FileDiscardReason::ExceedsHardCap {
            byte_len: meta.len(),
        });
    }
    None
}

fn should_prune_large_ignored_dir(
    abs_dir: &Path,
    threshold: usize,
    cancel: Option<&AtomicBool>,
) -> bool {
    if abs_dir.join(".ignore").is_file() {
        return false;
    }
    let mut count = 0usize;
    let mut stack = vec![abs_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return false;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for item_res in entries {
            let Ok(item) = item_res else {
                continue;
            };
            let Ok(ft) = item.file_type() else {
                continue;
            };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_file()
                && item
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(".ignore"))
            {
                return false;
            }
            count += 1;
            if count > threshold {
                return true;
            }
            if ft.is_dir() {
                stack.push(item.path());
            }
        }
    }
    false
}
