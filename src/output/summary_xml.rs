//! Strict XML serialization for `<context_summary>` and stacked `<directory_structure>` paths.

use std::fmt::Write as _;

use crate::domain::{
    DirectoryTree, FileDiscardReason, GitContextSnapshot, NodeKind, PrunedDir, UnselectedReason,
    format_token_count,
};
use crate::output::xml_escape::{push_escaped_xml_attr, push_escaped_xml_text};

const MAX_SUMMARY_PRUNED_DIRS: usize = 12;

/// Options controlling optional metadata sections inside strict XML output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XmlRenderOptions<'a> {
    /// Optional Git repository metadata rendered in `<git_context>`.
    pub git_context: Option<&'a GitContextSnapshot>,
    /// Optional normalized absolute root path emitted as `root_path` when opt-in `[p]` is active.
    pub abs_root_path: Option<&'a str>,
}

/// Renders `<context_summary>` and `<directory_structure>` into `out` for XML emission or sizing.
pub fn render_xml_header_sections(
    tree: &DirectoryTree,
    abs_root_path: Option<&str>,
    out: &mut String,
) {
    render_context_summary(tree, abs_root_path, out);
    render_stacked_directory_structure(tree, out);
}

fn render_context_summary(tree: &DirectoryTree, abs_root_path: Option<&str>, out: &mut String) {
    let metrics = tree.compute_metrics();
    let root_node = &tree.nodes()[DirectoryTree::ROOT.index()];
    let discarded_files = metrics.rollups[DirectoryTree::ROOT.index()].discarded_files;

    out.push_str("  <context_summary root=\"");
    push_escaped_xml_attr(out, &root_node.name);
    out.push('"');

    if let Some(abs_path) = abs_root_path {
        out.push_str(" root_path=\"");
        push_escaped_xml_attr(out, abs_path);
        out.push('"');
    }

    let _ = write!(
        out,
        " included_files=\"{}\" unselected_files=\"{}\" discarded_files=\"{discarded_files}\"",
        metrics.counts.read_files, metrics.counts.unread_files,
    );

    if !tree.pruned_dirs().is_empty() {
        let pruned = format_pruned_dirs_summary(tree.pruned_dirs());
        out.push_str(" pruned_dirs=\"");
        push_escaped_xml_attr(out, &pruned);
        out.push('"');
    }

    out.push_str(" />\n");
}

fn render_stacked_directory_structure(tree: &DirectoryTree, out: &mut String) {
    let NodeKind::Directory(root_dir) = &tree.nodes()[DirectoryTree::ROOT.index()].kind else {
        return;
    };
    if root_dir.children.is_empty() {
        out.push_str("  <directory_structure/>\n");
        return;
    }

    out.push_str("  <directory_structure>\n");
    let mut stack: Vec<_> = root_dir.children.iter().copied().rev().collect();
    while let Some(node_id) = stack.pop() {
        let node = &tree.nodes()[node_id.index()];
        match &node.kind {
            NodeKind::Directory(dir) => {
                if dir.children.is_empty() {
                    out.push_str("    ");
                    push_escaped_xml_text(out, &node.rel_path);
                    out.push_str("/\n");
                } else {
                    for &child in dir.children.iter().rev() {
                        stack.push(child);
                    }
                }
            }
            NodeKind::TextFile(file) => {
                out.push_str("    ");
                push_escaped_xml_text(out, &node.rel_path);
                if let Some(reason) = file.state.unselected_reason() {
                    out.push_str(" [unselected: ");
                    out.push_str(unselected_reason_label(reason));
                    out.push_str("] (");
                    out.push_str(&format_token_count(file.tokens));
                    out.push(')');
                    if file.secret_hits > 0 {
                        out.push_str(" [!]");
                    }
                }
                out.push('\n');
            }
            NodeKind::DiscardedFile(reason) => {
                out.push_str("    ");
                push_escaped_xml_text(out, &node.rel_path);
                out.push_str(" [");
                out.push_str(discard_reason_label(*reason));
                out.push_str("]\n");
            }
        }
    }
    out.push_str("  </directory_structure>\n");
}

fn format_pruned_dirs_summary(pruned: &[PrunedDir]) -> String {
    let shown = pruned.len().min(MAX_SUMMARY_PRUNED_DIRS);
    let mut out = String::new();
    for (idx, dir) in pruned.iter().take(shown).enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&dir.rel_path);
    }
    if pruned.len() > MAX_SUMMARY_PRUNED_DIRS {
        let rest = pruned.len().saturating_sub(MAX_SUMMARY_PRUNED_DIRS);
        let _ = write!(out, ", ... (+{rest} more)");
    }
    out
}

const fn unselected_reason_label(reason: UnselectedReason) -> &'static str {
    match reason {
        UnselectedReason::UserExcluded => "off",
        UnselectedReason::DotIgnore => ".ignore",
        UnselectedReason::GitIgnore => ".gitignore",
        UnselectedReason::ExceedsTokenThreshold => ">50k",
        UnselectedReason::PrunedDir => "pruned",
    }
}

const fn discard_reason_label(reason: FileDiscardReason) -> &'static str {
    match reason {
        FileDiscardReason::Binary => "binary",
        FileDiscardReason::InvalidUtf8 => "UTF8",
        FileDiscardReason::ExceedsHardCap { .. } => ">5MB",
        FileDiscardReason::Unreadable => "unreadable",
    }
}
