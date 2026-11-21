//! Iterative filesystem walker integrating dependency pruning and layered ignore rules.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf, Prefix};

use crate::config::defaults::{HARD_READ_BYTE_CAP, should_prune_dir};
use crate::domain::{
    DirectoryTree, NodeId, PrunedDirReason, TextFileData, TextFileState, TreeError,
};
use crate::scanner::file_reader::{ReadFileOutcome, read_bounded_utf8};
use crate::scanner::ignore_rules::{IgnoreDecision, IgnoreRuleStack};

/// Runtime configuration controlling directory scanning behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanConfig {
    /// Hard file size cap in bytes passed to [`read_bounded_utf8`].
    pub max_file_bytes: u64,
    /// Whether `.ignore` and `.gitignore` patterns match case-insensitively.
    pub case_insensitive_ignores: bool,
    /// Whether directories matched by ignore rules start expanded in the TUI.
    pub expand_ignored_dirs: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: HARD_READ_BYTE_CAP,
            case_insensitive_ignores: cfg!(windows),
            expand_ignored_dirs: false,
        }
    }
}

/// Complete output produced by scanning a directory tree on disk.
#[derive(Debug)]
pub struct ScanResult {
    /// Arena tree containing directories, discarded files, and text file metadata.
    pub tree: DirectoryTree,
    /// Loaded `UTF-8` payloads keyed by [`NodeId`] for readable text files.
    pub contents: HashMap<NodeId, String>,
}

/// Fatal errors preventing initialization or root traversal of a target directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// Target path does not exist or is not a directory.
    NotADirectory(PathBuf),
    /// Failed to read entries from the root directory.
    ReadDirFailed {
        /// Path that failed to enumerate.
        path: PathBuf,
        /// Underlying OS error message.
        reason: String,
    },
    /// Internal tree construction invariant violation.
    TreeStructure(TreeError),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADirectory(path) => {
                write!(f, "target is not a readable directory: {}", path.display())
            }
            Self::ReadDirFailed { path, reason } => {
                write!(f, "failed to read directory {}: {reason}", path.display())
            }
            Self::TreeStructure(err) => write!(f, "tree structure error: {err}"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<TreeError> for ScanError {
    fn from(err: TreeError) -> Self {
        Self::TreeStructure(err)
    }
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

pub(super) struct SortedEntry {
    pub(super) name: String,
    lower_name: String,
    pub(super) abs_path: PathBuf,
    pub(super) is_dir: bool,
}

/// Walks `root` iteratively, pruning dependency/VCS folders and evaluating `.ignore` > `.gitignore`.
///
/// # Errors
///
/// Returns [`ScanError`] if `root` is not a readable directory or root enumeration fails.
pub fn scan_directory(root: &Path, config: &ScanConfig) -> Result<ScanResult, ScanError> {
    if !root.is_dir() {
        return Err(ScanError::NotADirectory(root.to_path_buf()));
    }

    let mut tree = DirectoryTree::new(resolve_root_name(root));
    let mut contents = HashMap::new();
    let mut ignore_stack = IgnoreRuleStack::with_case_sensitivity(config.case_insensitive_ignores);
    let mut work_stack = vec![WalkStep::Enter {
        node_id: DirectoryTree::ROOT,
        abs_dir: root.to_path_buf(),
        rel_dir: String::new(),
    }];

    while let Some(step) = work_stack.pop() {
        match step {
            WalkStep::Leave => ignore_stack.pop_dir(),
            WalkStep::Enter {
                node_id,
                abs_dir,
                rel_dir,
            } => {
                ignore_stack.push_dir(&abs_dir);
                work_stack.push(WalkStep::Leave);

                let subdirs = process_directory(
                    node_id,
                    &abs_dir,
                    &rel_dir,
                    config,
                    &ignore_stack,
                    &mut tree,
                    &mut contents,
                )?;
                for sub in subdirs.into_iter().rev() {
                    work_stack.push(WalkStep::Enter {
                        node_id: sub.node_id,
                        abs_dir: sub.abs_dir,
                        rel_dir: sub.rel_dir,
                    });
                }
            }
        }
    }

    Ok(ScanResult { tree, contents })
}

/// Resolves a concise display label for a scanned root directory, including Windows drive roots.
#[must_use]
pub fn resolve_root_name(root: &Path) -> String {
    let resolved = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if let Some(name) = resolved.file_name().or_else(|| root.file_name()) {
        let text = name.to_string_lossy();
        if !text.is_empty() {
            return text.into_owned();
        }
    }
    extract_root_label(&resolved)
        .or_else(|| extract_root_label(root))
        .unwrap_or_else(|| "workspace".to_owned())
}

fn process_directory(
    parent_id: NodeId,
    abs_dir: &Path,
    rel_dir: &str,
    config: &ScanConfig,
    ignore_stack: &IgnoreRuleStack,
    tree: &mut DirectoryTree,
    contents: &mut HashMap<NodeId, String>,
) -> Result<Vec<PendingSubdir>, ScanError> {
    let entries = match read_sorted_entries(abs_dir) {
        Ok(list) => list,
        Err(err) if parent_id == DirectoryTree::ROOT => return Err(err),
        // Skip protected or transient subdirectories without aborting the rest of the tree.
        Err(_) => return Ok(Vec::new()),
    };

    let mut subdirs = Vec::new();
    for entry in entries {
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
            let is_expanded = config.expand_ignored_dirs
                || ignore_stack.evaluate(&entry.abs_path, true) == IgnoreDecision::Included;
            let dir_id = tree.add_dir(parent_id, &entry.name, is_expanded)?;
            subdirs.push(PendingSubdir {
                node_id: dir_id,
                abs_dir: entry.abs_path,
                rel_dir: child_rel,
            });
        } else {
            let decision = ignore_stack.evaluate(&entry.abs_path, false);
            match read_bounded_utf8(&entry.abs_path, config.max_file_bytes) {
                ReadFileOutcome::Text { byte_len, content } => {
                    let file_id = tree.add_text_file(
                        parent_id,
                        &entry.name,
                        TextFileData {
                            byte_len,
                            tokens: 0,
                            secret_hits: 0,
                            state: TextFileState::new(decision.to_natural_selection()),
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

pub(super) fn read_sorted_entries(dir: &Path) -> Result<Vec<SortedEntry>, ScanError> {
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

fn extract_root_label(path: &Path) -> Option<String> {
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            Some(format!("{}:", char::from(letter)))
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => Some(format!(
            "\\\\{}\\{}",
            server.to_string_lossy(),
            share.to_string_lossy()
        )),
        Prefix::DeviceNS(dev) | Prefix::Verbatim(dev) => Some(dev.to_string_lossy().into_owned()),
    }
}
