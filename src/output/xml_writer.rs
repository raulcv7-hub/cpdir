//! Strict XML 1.0 document assembly for directory structure, git context, and selected files.

use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasher;

use crate::domain::{DirectoryTree, GitContextSnapshot, NodeId, NodeKind};
use crate::output::git_xml::render_git_context;
use crate::output::xml_escape::{push_escaped_xml_attr, push_escaped_xml_text};

/// Errors prevented while assembling the final XML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlWriteError {
    /// A selected `TextFile` node has no loaded text payload in the content map.
    MissingFileContent {
        /// Relative path of the missing file payload.
        rel_path: String,
    },
}

impl fmt::Display for XmlWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFileContent { rel_path } => {
                write!(
                    f,
                    "missing loaded UTF-8 content for selected file: {rel_path}"
                )
            }
        }
    }
}

impl std::error::Error for XmlWriteError {}

/// Renders the entire `DirectoryTree`, optional Git context, and selected files into strict XML 1.0.
///
/// # Errors
///
/// Returns [`XmlWriteError::MissingFileContent`] if any selected [`NodeKind::TextFile`] lacks
/// an entry in `contents`.
pub fn render_strict_xml<S: BuildHasher>(
    tree: &DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
    git_context: Option<&GitContextSnapshot>,
) -> Result<String, XmlWriteError> {
    let estimated_bytes = estimate_output_bytes(tree);
    let mut xml = String::with_capacity(estimated_bytes);

    xml.push_str("<cpdir_context>\n");
    xml.push_str("  <directory_structure>\n");
    render_directory_structure(tree, &mut xml);
    xml.push_str("  </directory_structure>\n");

    if let Some(git) = git_context {
        render_git_context(git, &mut xml);
    }

    xml.push_str("  <files>\n");
    render_selected_files(tree, contents, &mut xml)?;
    xml.push_str("  </files>\n");
    xml.push_str("</cpdir_context>\n");

    Ok(xml)
}

fn estimate_output_bytes(tree: &DirectoryTree) -> usize {
    let payload_sum: usize = tree
        .nodes()
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::TextFile(f) if f.state.is_selected() => {
                Some(usize::try_from(f.byte_len).unwrap_or(0))
            }
            _ => None,
        })
        .sum();
    payload_sum.saturating_add(4_096)
}

fn render_directory_structure(tree: &DirectoryTree, out: &mut String) {
    let mut stack = vec![(DirectoryTree::ROOT, 2usize)];
    while let Some((node_id, indent)) = stack.pop() {
        let node = &tree.nodes()[node_id.index()];
        push_indent(out, indent);
        push_escaped_xml_text(out, &node.name);
        if let NodeKind::Directory(dir) = &node.kind {
            out.push('/');
            for &child in dir.children.iter().rev() {
                stack.push((child, indent + 1));
            }
        }
        out.push('\n');
    }
}

fn render_selected_files<S: BuildHasher>(
    tree: &DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
    out: &mut String,
) -> Result<(), XmlWriteError> {
    let mut stack = vec![DirectoryTree::ROOT];
    while let Some(node_id) = stack.pop() {
        let node = &tree.nodes()[node_id.index()];
        match &node.kind {
            NodeKind::Directory(dir) => {
                for &child in dir.children.iter().rev() {
                    stack.push(child);
                }
            }
            NodeKind::TextFile(file) if file.state.is_selected() => {
                let Some(content) = contents.get(&node_id) else {
                    return Err(XmlWriteError::MissingFileContent {
                        rel_path: node.rel_path.clone(),
                    });
                };
                out.push_str("    <file path=\"");
                push_escaped_xml_attr(out, &node.rel_path);
                out.push_str("\">");
                push_escaped_xml_text(out, content);
                out.push_str("</file>\n");
            }
            NodeKind::TextFile(_) | NodeKind::DiscardedFile(_) => {}
        }
    }
    Ok(())
}

fn push_indent(out: &mut String, levels: usize) {
    for _ in 0..levels {
        out.push_str("  ");
    }
}
