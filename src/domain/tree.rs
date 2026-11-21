//! Arena-backed directory tree data structures enforcing parent-before-child invariants.

use std::fmt;

use crate::domain::selection::{FileDiscardReason, PrunedDirReason, TextFileState};

/// Typed handle referencing a node inside [`DirectoryTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) usize);

impl NodeId {
    /// Returns the zero-based arena index for array-backed lookups.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Structural errors prevented when building a [`DirectoryTree`].
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

/// Single entry inside the arena-backed [`DirectoryTree`].
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

/// Arena-backed repository tree guaranteeing parent indices precede all descendants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryTree {
    pub(crate) nodes: Vec<TreeNode>,
    pub(crate) pruned_dirs: Vec<PrunedDir>,
}

impl DirectoryTree {
    /// Root node handle (always index `0`).
    pub const ROOT: NodeId = NodeId(0);

    /// Creates a new tree initialized with an expanded root directory.
    #[must_use]
    pub fn new(root_name: impl Into<String>) -> Self {
        let root = TreeNode {
            parent: None,
            name: root_name.into(),
            rel_path: String::new(),
            kind: NodeKind::Directory(DirectoryData {
                children: Vec::new(),
                is_expanded: true,
                folder_override: None,
            }),
        };
        Self {
            nodes: vec![root],
            pruned_dirs: Vec::new(),
        }
    }

    /// Returns an immutable reference to all nodes in arena order.
    #[must_use]
    pub fn nodes(&self) -> &[TreeNode] {
        &self.nodes
    }

    /// Returns a reference to a single node if `id` is valid.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&TreeNode> {
        self.nodes.get(id.0)
    }

    /// Returns a mutable reference to a single node if `id` is valid.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut TreeNode> {
        self.nodes.get_mut(id.0)
    }

    /// Returns all dependency/VCS directories pruned from the interactive tree.
    #[must_use]
    pub fn pruned_dirs(&self) -> &[PrunedDir] {
        &self.pruned_dirs
    }

    /// Records a pruned directory for visibility inside the Discarded inspector.
    pub fn record_pruned_dir(&mut self, rel_path: impl Into<String>, reason: PrunedDirReason) {
        self.pruned_dirs.push(PrunedDir {
            rel_path: rel_path.into(),
            reason,
        });
    }

    /// Appends a child subdirectory under `parent` and returns its [`NodeId`].
    ///
    /// # Errors
    ///
    /// Returns [`TreeError::EmptyName`] if `name` is empty, or [`TreeError::InvalidParent`]
    /// if `parent` does not point to an existing directory node.
    pub fn add_dir(
        &mut self,
        parent: NodeId,
        name: impl Into<String>,
        is_expanded: bool,
    ) -> Result<NodeId, TreeError> {
        self.insert_child(
            parent,
            name.into(),
            NodeKind::Directory(DirectoryData {
                children: Vec::new(),
                is_expanded,
                folder_override: None,
            }),
        )
    }

    /// Appends a non-selectable discarded file under `parent`.
    ///
    /// # Errors
    ///
    /// Returns [`TreeError`] if `name` is empty or `parent` is not a valid directory.
    pub fn add_discarded_file(
        &mut self,
        parent: NodeId,
        name: impl Into<String>,
        reason: FileDiscardReason,
    ) -> Result<NodeId, TreeError> {
        self.insert_child(parent, name.into(), NodeKind::DiscardedFile(reason))
    }

    /// Appends a readable `UTF-8` text file under `parent`.
    ///
    /// # Errors
    ///
    /// Returns [`TreeError`] if `name` is empty or `parent` is not a valid directory.
    pub fn add_text_file(
        &mut self,
        parent: NodeId,
        name: impl Into<String>,
        data: TextFileData,
    ) -> Result<NodeId, TreeError> {
        self.insert_child(parent, name.into(), NodeKind::TextFile(data))
    }

    fn insert_child(
        &mut self,
        parent: NodeId,
        name: String,
        kind: NodeKind,
    ) -> Result<NodeId, TreeError> {
        if name.is_empty() {
            return Err(TreeError::EmptyName);
        }
        let parent_rel = match self.nodes.get(parent.0) {
            Some(node) if matches!(node.kind, NodeKind::Directory(_)) => node.rel_path.clone(),
            _ => return Err(TreeError::InvalidParent(parent)),
        };

        let rel_path = if parent_rel.is_empty() {
            name.clone()
        } else {
            format!("{parent_rel}/{name}")
        };
        let child_id = NodeId(self.nodes.len());

        self.nodes.push(TreeNode {
            parent: Some(parent),
            name,
            rel_path,
            kind,
        });

        if let Some(NodeKind::Directory(dir)) = self.nodes.get_mut(parent.0).map(|n| &mut n.kind) {
            dir.children.push(child_id);
        }

        Ok(child_id)
    }
}
