//! Core arena node handles, errors, and payload classifications for [`crate::domain::DirectoryTree`].

use std::fmt;

use crate::domain::selection::{FileDiscardReason, PrunedDirReason, TextFileState};

/// Typed handle referencing a node inside [`crate::domain::DirectoryTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) usize);

impl NodeId {
    /// Returns the zero-based arena index for array-backed lookups.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Structural errors prevented when building a [`crate::domain::DirectoryTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    /// Attempted to attach a child under a non-existent or non-directory parent.
    InvalidParent(NodeId),
    /// Attempted to add an entry with an empty name.
    EmptyName,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParent(id) => {
                write!(f, "node {} is not a valid parent directory", id.index())
            }
            Self::EmptyName => write!(f, "tree entry name cannot be empty"),
        }
    }
}

impl std::error::Error for TreeError {}

/// Metadata specific to directory entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryData {
    /// Direct child node handles in deterministic display order.
    pub children: Vec<NodeId>,
    /// Whether this directory is currently expanded in the TUI tree.
    pub is_expanded: bool,
    /// Explicit folder-level exclusion override (`Some(false)`) propagated to new files.
    pub folder_override: Option<bool>,
}

/// Metadata specific to readable `UTF-8` text files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFileData {
    /// File byte size on disk.
    pub byte_len: u64,
    /// Exact token count assigned by the tokenizer.
    pub tokens: usize,
    /// Number of potential secret findings detected inside the file.
    pub secret_hits: usize,
    /// Algebraic inclusion state combining natural defaults and user overrides.
    pub state: TextFileState,
}

/// Classification separating directories, discarded files, and readable text files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// Directory node containing child handles.
    Directory(DirectoryData),
    /// Non-text, oversized, or corrupt file included in `<directory_structure>` only.
    DiscardedFile(FileDiscardReason),
    /// Readable `UTF-8` file eligible for selection and `<files>` output.
    TextFile(TextFileData),
}

/// Single entry inside the arena-backed [`crate::domain::DirectoryTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    /// Parent node handle (`None` strictly for the root directory).
    pub parent: Option<NodeId>,
    /// File or directory basename.
    pub name: String,
    /// Normalized forward-slash path relative to workspace root (`""` for root).
    pub rel_path: String,
    /// Node variant and domain payload.
    pub kind: NodeKind,
}

/// Dependency or VCS directory pruned before populating the interactive tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunedDir {
    /// Normalized forward-slash relative path of the pruned directory.
    pub rel_path: String,
    /// Reason the subtree was pruned.
    pub reason: PrunedDirReason,
}
