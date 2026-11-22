//! Width-aware tree row formatting with branch guides and right-pinned metadata.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::domain::{
    DirectoryTree, FileDiscardReason, FolderSelectionState, NodeId, NodeKind, UnselectedReason,
    VisibleRow, WeightTier,
};
use crate::tui::state::TuiAppState;
use crate::tui::theme::{
    NodeVisualKind, cursor_glyph_style, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, secret_marker_style, tree_guide_style,
};

struct RowParts {
    prefix_spans: Vec<Span<'static>>,
    label: String,
    label_style: Style,
    right_spans: Vec<Span<'static>>,
}

/// Builds a single tree row preserving right-side token/reason/`[!]` badges within `max_width`.
pub(super) fn build_tree_row_line(
    state: &TuiAppState,
    row: VisibleRow,
    is_cursor: bool,
    max_width: usize,
) -> Line<'static> {
    let mut parts = prepare_row_parts(state, row, is_cursor);
    let prefix_w: usize = parts.prefix_spans.iter().map(Span::width).sum();
    let right_w: usize = parts.right_spans.iter().map(Span::width).sum();
    let avail_label_w = max_width.saturating_sub(prefix_w + right_w);
    let fitted_label = truncate_to_display_width(&parts.label, avail_label_w);

    parts
        .prefix_spans
        .push(Span::styled(fitted_label, parts.label_style));
    parts.prefix_spans.extend(parts.right_spans);

    let mut line = Line::from(parts.prefix_spans);
    if is_cursor {
        line = line.style(cursor_row_style());
    }
    line
}

fn prepare_row_parts(state: &TuiAppState, row: VisibleRow, is_cursor: bool) -> RowParts {
    let node = &state.tree().nodes()[row.node_id.index()];
    let rollup = state.metrics().rollups[row.node_id.index()];
    let mut prefix_spans = Vec::new();

    if is_cursor {
        prefix_spans.push(Span::styled("▶ ", cursor_glyph_style()));
    } else {
        prefix_spans.push(Span::raw("  "));
    }
    let guides = build_branch_guides(state.tree(), row.node_id, row.depth);
    if !guides.is_empty() {
        prefix_spans.push(Span::styled(guides, tree_guide_style()));
    }

    let clean_name = sanitize_tui_label(&node.name);
    let mut right_spans = Vec::new();

    match &node.kind {
        NodeKind::Directory(dir) => {
            let fold = if dir.is_expanded { "▾ " } else { "▸ " };
            let checkbox = match rollup.folder_selection {
                FolderSelectionState::AllSelected => "[x] ",
                FolderSelectionState::Partial => "[-] ",
                FolderSelectionState::NoneSelected => "[ ] ",
                FolderSelectionState::NoTextFiles => "[·] ",
            };
            let active = rollup.selected_files > 0;
            let tier = rollup.display_weight_tier(active);
            let style = node_label_style(tier, NodeVisualKind::Directory { active });

            prefix_spans.push(Span::styled(format!("{fold}{checkbox}"), style));
            let weight_text = format_dir_weight(rollup.selected_tokens, rollup.total_tokens);
            if !weight_text.is_empty() {
                right_spans.push(Span::styled(weight_text, style));
            }
            if rollup.has_any_secret {
                right_spans.push(secret_span(rollup.has_active_secret));
            }

            RowParts {
                prefix_spans,
                label: format!("{clean_name}/"),
                label_style: style,
                right_spans,
            }
        }
        NodeKind::TextFile(file) => {
            let active = file.state.is_selected();
            let checkbox = if active { "[x] " } else { "[ ] " };
            let tier = WeightTier::from_tokens(file.tokens);
            let kind = if file.secret_hits > 0 {
                NodeVisualKind::SecretTextFile { active }
            } else {
                NodeVisualKind::TextFile { active }
            };
            let style = node_label_style(tier, kind);
            let tok_str = format_token_count(file.tokens);

            prefix_spans.push(Span::styled(format!("  {checkbox}"), style));
            right_spans.push(Span::styled(format!(" ({tok_str})"), style));
            if let Some(reason) = file.state.unselected_reason() {
                right_spans.push(Span::styled(
                    format!(" [{}]", unselected_tag(reason)),
                    Style::new()
                        .fg(Color::Rgb(110, 115, 125))
                        .add_modifier(Modifier::DIM),
                ));
            }
            if file.secret_hits > 0 {
                right_spans.push(secret_span(active));
            }

            RowParts {
                prefix_spans,
                label: clean_name,
                label_style: style,
                right_spans,
            }
        }
        NodeKind::DiscardedFile(reason) => {
            let tag = discard_tag(*reason);
            let style = node_label_style(WeightTier::Light, NodeVisualKind::Discarded);
            prefix_spans.push(Span::styled(format!("  [{tag}] "), style));

            RowParts {
                prefix_spans,
                label: clean_name,
                label_style: style,
                right_spans,
            }
        }
    }
}

fn build_branch_guides(tree: &DirectoryTree, node_id: NodeId, depth: usize) -> String {
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

fn truncate_to_display_width(text: &str, max_cells: usize) -> String {
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

fn format_dir_weight(selected: usize, total: usize) -> String {
    if total == 0 {
        return String::new();
    }
    if selected == total {
        return format!(" ({})", format_token_count(selected));
    }
    format!(
        " ({}/{})",
        format_token_count(selected),
        format_token_count(total)
    )
}

fn secret_span(active: bool) -> Span<'static> {
    if active {
        Span::styled(" [!]", secret_marker_style())
    } else {
        Span::styled(
            " [!]",
            Style::new()
                .fg(Color::Rgb(150, 75, 95))
                .add_modifier(Modifier::DIM),
        )
    }
}

const fn unselected_tag(reason: UnselectedReason) -> &'static str {
    match reason {
        UnselectedReason::UserExcluded => "off",
        UnselectedReason::DotIgnore => ".ignore",
        UnselectedReason::GitIgnore => ".gitignore",
        UnselectedReason::ExceedsTokenThreshold => ">50k",
        UnselectedReason::PrunedDir => "pruned",
    }
}

const fn discard_tag(reason: FileDiscardReason) -> &'static str {
    match reason {
        FileDiscardReason::Binary => "BIN",
        FileDiscardReason::InvalidUtf8 => "UTF8",
        FileDiscardReason::ExceedsHardCap { .. } => ">5MB",
        FileDiscardReason::Unreadable => "ERR",
    }
}
