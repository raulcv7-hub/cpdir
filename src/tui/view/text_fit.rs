//! Unicode display-width truncation and tree branch guide rendering helpers.

use ratatui::text::Span;

use crate::domain::{DirectoryTree, NodeId, NodeKind};

pub(super) fn build_branch_guides(tree: &DirectoryTree, node_id: NodeId, depth: usize) -> String {
    if depth == 0 {
        return String::new();
    }
    let mut chain_is_last = Vec::with_capacity(depth);
    let mut curr = node_id;
    while let Some(node) = tree.get(curr) {
        let Some(parent_id) = node.parent else {
            break;
        };
        let is_last = match &tree.nodes()[parent_id.index()].kind {
            NodeKind::Directory(dir) => dir.children.last() == Some(&curr),
            _ => true,
        };
        chain_is_last.push(is_last);
        curr = parent_id;
    }

    let mut out = String::with_capacity(depth * 2);
    let total = chain_is_last.len();
    for (idx, &is_last) in chain_is_last.iter().rev().enumerate() {
        if idx + 1 == total {
            out.push_str(if is_last { "└─" } else { "├─" });
        } else {
            out.push_str(if is_last { "  " } else { "│ " });
        }
    }
    out
}

pub(super) fn truncate_to_display_width(text: &str, max_cells: usize) -> String {
    let total_w = Span::raw(text).width();
    if total_w <= max_cells {
        return text.to_owned();
    }
    if max_cells == 0 {
        return String::new();
    }
    if max_cells == 1 {
        return "…".to_owned();
    }

    let target_w = max_cells.saturating_sub(1);
    let mut accumulated_w = 0usize;
    let mut byte_idx = 0usize;

    for (idx, ch) in text.char_indices() {
        let mut buf = [0u8; 4];
        let ch_str: &str = ch.encode_utf8(&mut buf);
        let ch_w = Span::raw(ch_str).width();
        if accumulated_w.saturating_add(ch_w) > target_w {
            break;
        }
        accumulated_w += ch_w;
        byte_idx = idx + ch.len_utf8();
    }

    let mut out = String::with_capacity(byte_idx + 3);
    out.push_str(&text[..byte_idx]);
    out.push('…');
    out
}
