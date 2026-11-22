//! Arena-backed directory tree container enforcing parent-before-child invariants.

use std::collections::HashMap;

use crate::domain::findings::SecretFinding;
pub use crate::domain::node::{
    DirectoryData, NodeId, NodeKind, PrunedDir, TextFileData, TreeError, TreeNode,
};
use crate::domain::selection::{FileDiscardReason, PrunedDirReason};

/// Arena-backed repository tree guaranteeing parent indices precede all descendants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryTree {
    pub(crate) nodes: Vec<TreeNode>,
    pub(crate) pruned_dirs: Vec<PrunedDir>,
    pub(crate) secret_findings: HashMap<NodeId, Vec<SecretFinding>>,
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
            secret_findings: HashMap::new(),
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

    /// Stores detailed secret findings for text node `id` and synchronizes its hit count.
    pub fn set_secret_findings(&mut self, id: NodeId, findings: Vec<SecretFinding>) {
        if let Some(node) = self.nodes.get_mut(id.0)
            && let NodeKind::TextFile(file) = &mut node.kind
        {
            file.secret_hits = findings.len();
            if findings.is_empty() {
                self.secret_findings.remove(&id);
            } else {
                self.secret_findings.insert(id, findings);
            }
        }
    }

    /// Returns detailed secret findings recorded for `id`, or an empty slice if none exist.
    #[must_use]
    pub fn secret_findings(&self, id: NodeId) -> &[SecretFinding] {
        self.secret_findings.get(&id).map_or(&[], Vec::as_slice)
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
