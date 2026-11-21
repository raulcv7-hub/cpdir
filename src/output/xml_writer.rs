//! Strict XML 1.0 serialization with control-character sanitization and zero-loss escaping.

use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::hash::BuildHasher;

use crate::domain::{DirectoryTree, NodeId, NodeKind};

/// Single commit summary rendered inside `<git_context>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommitEntry {
    /// Abbreviated commit hash.
    pub short_hash: String,
    /// Commit title line.
    pub subject: String,
    /// Optional truncated commit body.
    pub body: Option<String>,
}

/// File modification frequency entry rendered inside `<hotspots>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHotspotEntry {
    /// Forward-slash relative path inside the repository.
    pub rel_path: String,
    /// Number of commits touching this file within the sampled history window.
    pub commits: usize,
}

/// Optional Git repository context snapshot rendered when enabled (`[g]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitContextSnapshot {
    /// Active branch summary (including ahead/behind status when available).
    pub branch: String,
    /// Short `git status` lines for the current working tree.
    pub working_tree_lines: Vec<String>,
    /// Recent commits ordered newest-first.
    pub recent_commits: Vec<GitCommitEntry>,
    /// Most frequently modified files in recent history.
    pub hotspots: Vec<GitHotspotEntry>,
}

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

fn render_git_context(git: &GitContextSnapshot, out: &mut String) {
    out.push_str("  <git_context>\n");
    out.push_str("    <branch>");
    push_escaped_xml_text(out, &git.branch);
    out.push_str("</branch>\n");

    if git.working_tree_lines.is_empty() {
        out.push_str("    <working_tree/>\n");
    } else {
        out.push_str("    <working_tree>\n");
        for line in &git.working_tree_lines {
            out.push_str("      ");
            push_escaped_xml_text(out, line);
            out.push('\n');
        }
        out.push_str("    </working_tree>\n");
    }

    let _ = writeln!(
        out,
        "    <recent_commits count=\"{}\">",
        git.recent_commits.len()
    );
    for commit in &git.recent_commits {
        out.push_str("      ");
        push_escaped_xml_text(out, &commit.short_hash);
        out.push_str(" | ");
        push_escaped_xml_text(out, &commit.subject);
        out.push('\n');
        if let Some(body) = &commit.body {
            for body_line in body.lines() {
                out.push_str("        ");
                push_escaped_xml_text(out, body_line);
                out.push('\n');
            }
        }
    }
    out.push_str("    </recent_commits>\n");

    if !git.hotspots.is_empty() {
        out.push_str("    <hotspots>\n");
        for spot in &git.hotspots {
            out.push_str("      ");
            push_escaped_xml_text(out, &spot.rel_path);
            let _ = writeln!(out, " ({} commits)", spot.commits);
        }
        out.push_str("    </hotspots>\n");
    }
    out.push_str("  </git_context>\n");
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

fn push_escaped_xml_text(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#xD;"),
            _ if is_valid_xml10_char(ch) => out.push(ch),
            _ => out.push('\u{FFFD}'),
        }
    }
}

fn push_escaped_xml_attr(out: &mut String, attr: &str) {
    for ch in attr.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\r' => out.push_str("&#xD;"),
            '\n' => out.push_str("&#xA;"),
            '\t' => out.push_str("&#x9;"),
            _ if is_valid_xml10_char(ch) => out.push(ch),
            _ => out.push('\u{FFFD}'),
        }
    }
}

const fn is_valid_xml10_char(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n'
            | '\r'
            | '\u{0020}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
    )
}
