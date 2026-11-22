//! Shared scan, analysis, tree lookup, and mock clipboard helpers reused across test suites.

use std::cell::RefCell;

use cpdir::application::analyze_tree_contents;
use cpdir::domain::{DirectoryTree, NodeId, NodeKind};
use cpdir::output::{ClipboardError, ClipboardWriter};
use cpdir::scanner::{ScanConfig, ScanResult, scan_directory};

use crate::common::workspace::WorkspaceBuilder;

/// In-memory [`ClipboardWriter`] recording every copy payload or injecting an error.
#[derive(Default)]
pub(crate) struct RecordingClipboard {
    pub(crate) copied: RefCell<Vec<String>>,
    pub(crate) fail_with: Option<ClipboardError>,
}

impl ClipboardWriter for RecordingClipboard {
    fn copy_text(&self, text: &str) -> Result<(), ClipboardError> {
        if let Some(err) = &self.fail_with {
            return Err(err.clone());
        }
        self.copied.borrow_mut().push(text.to_owned());
        Ok(())
    }
}

/// Scans `ws` with default config and populates token counts and secret findings.
pub(crate) fn scan_and_analyze(ws: &WorkspaceBuilder) -> ScanResult {
    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan succeeds");
    analyze_tree_contents(&mut scan.tree, &scan.contents).expect("analysis succeeds");
    scan
}

/// Resolves the [`NodeId`] matching `rel_path` anywhere in `tree`, even under collapsed folders.
pub(crate) fn find_node_id(tree: &DirectoryTree, rel_path: &str) -> NodeId {
    let mut stack = vec![DirectoryTree::ROOT];
    while let Some(id) = stack.pop() {
        let node = &tree.nodes()[id.index()];
        if node.rel_path == rel_path {
            return id;
        }
        if let NodeKind::Directory(dir) = &node.kind {
            stack.extend(dir.children.iter().copied());
        }
    }
    panic!("could not resolve NodeId for {rel_path}")
}
