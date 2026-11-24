//! Public full-workspace scan facade, parallel file reading, and Windows root label resolution.

use std::collections::HashMap;
use std::fmt;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::atomic::AtomicBool;

use rayon::prelude::*;

use crate::config::limits::{DEFAULT_IGNORED_DIR_PRUNE_THRESHOLD, HARD_READ_BYTE_CAP};
use crate::domain::{DirectoryTree, NodeId, NodeKind, TreeError};
use crate::scanner::file_reader::{ReadFileOutcome, read_bounded_utf8};
use crate::scanner::ignore_rules::IgnoreRuleStack;
use crate::scanner::traversal::{
    DiscoveredFile, DiscoveryOutput, DiscoveryProgress, WalkPolicy, WalkRequest, discover_tree,
};

const PARALLEL_READ_MIN_FILES: usize = 8;

/// Runtime configuration controlling directory scanning behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanConfig {
    /// Hard file size cap in bytes passed to [`crate::scanner::read_bounded_utf8`].
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

/// Walks `root` iteratively, pruning dependency/VCS/massive-ignored folders and reading `UTF-8` files.
///
/// # Errors
///
/// Returns [`ScanError`] if `root` is not a readable directory or root enumeration fails.
pub fn scan_directory(root: &Path, config: &ScanConfig) -> Result<ScanResult, ScanError> {
    let mut out = discover_workspace_entries(root, config, None, &mut |_| {})?;
    let contents = read_discovered_files(&mut out.tree, &out.files, config.max_file_bytes);
    Ok(ScanResult {
        tree: out.tree,
        contents,
    })
}

pub(crate) fn discover_workspace_entries(
    root: &Path,
    config: &ScanConfig,
    cancel: Option<&AtomicBool>,
    on_progress: &mut dyn FnMut(DiscoveryProgress),
) -> Result<DiscoveryOutput, ScanError> {
    if !root.is_dir() {
        return Err(ScanError::NotADirectory(root.to_path_buf()));
    }

    discover_tree(
        WalkRequest {
            root_abs: root,
            root_name: &resolve_root_name(root),
            ignore_stack: IgnoreRuleStack::with_case_sensitivity(config.case_insensitive_ignores),
            config,
            policy: WalkPolicy {
                max_entries: None,
                ignored_dir_prune_threshold: Some(DEFAULT_IGNORED_DIR_PRUNE_THRESHOLD),
                rel_prefix: "",
                mark_pruned: false,
                expand_included_dirs: true,
                expand_ignored_dirs: config.expand_ignored_dirs,
            },
            cancel,
        },
        on_progress,
    )
}

pub(crate) fn read_discovered_files(
    tree: &mut DirectoryTree,
    files: &[DiscoveredFile],
    max_file_bytes: u64,
) -> HashMap<NodeId, String> {
    let outcomes: Vec<(NodeId, ReadFileOutcome)> = if files.len() < PARALLEL_READ_MIN_FILES {
        files
            .iter()
            .map(|f| (f.node_id, read_bounded_utf8(&f.abs_path, max_file_bytes)))
            .collect()
    } else {
        files
            .par_iter()
            .map(|f| (f.node_id, read_bounded_utf8(&f.abs_path, max_file_bytes)))
            .collect()
    };

    let mut contents = HashMap::new();
    for (id, outcome) in outcomes {
        if let Some(node) = tree.get_mut(id) {
            match outcome {
                ReadFileOutcome::Text { byte_len, content } => {
                    if let NodeKind::TextFile(file) = &mut node.kind {
                        file.byte_len = byte_len;
                    }
                    contents.insert(id, content);
                }
                ReadFileOutcome::Discarded(reason) => {
                    node.kind = NodeKind::DiscardedFile(reason);
                }
            }
        }
    }
    contents
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
