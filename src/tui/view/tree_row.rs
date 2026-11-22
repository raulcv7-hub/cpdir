//! Width-aware tree row formatting with right-pinned token and status badges.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::domain::{
    FileDiscardReason, FolderSelectionState, NodeKind, TextFileData, VisibleRow, WeightTier,
};
use crate::tui::state::TuiAppState;
use crate::tui::view::badges::{discard_tag, secret_span, unselected_tag};
use crate::tui::view::text_fit::{build_branch_guides, truncate_to_display_width};
use crate::tui::view::theme::{
    NodeVisualKind, cursor_glyph_style, cursor_row_style, format_token_count, node_label_style,
    sanitize_tui_label, tree_guide_style,
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
    let mut prefix_spans = vec![if is_cursor {
        Span::styled("▶ ", cursor_glyph_style())
    } else {
        Span::raw("  ")
    }];

    let guides = build_branch_guides(state.tree(), row.node_id, row.depth);
    if !guides.is_empty() {
        prefix_spans.push(Span::styled(guides, tree_guide_style()));
    }

    let clean_name = sanitize_tui_label(&node.name);
    match &node.kind {
        NodeKind::Directory(dir) => dir_row_parts(
            prefix_spans,
            &clean_name,
            dir.is_expanded,
            state.metrics().rollups[row.node_id.index()],
        ),
        NodeKind::TextFile(file) => text_row_parts(prefix_spans, clean_name, file),
        NodeKind::DiscardedFile(reason) => discarded_row_parts(prefix_spans, clean_name, *reason),
    }
}

fn dir_row_parts(
    mut prefix_spans: Vec<Span<'static>>,
    clean_name: &str,
    is_expanded: bool,
    rollup: crate::domain::NodeRollup,
) -> RowParts {
    let fold = if is_expanded { "▾ " } else { "▸ " };
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
    let mut right_spans = Vec::new();
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

fn text_row_parts(
    mut prefix_spans: Vec<Span<'static>>,
    clean_name: String,
    file: &TextFileData,
) -> RowParts {
    let active = file.state.is_selected();
    let checkbox = if active { "[x] " } else { "[ ] " };
    let tier = WeightTier::from_tokens(file.tokens);
    let kind = if file.secret_hits > 0 {
        NodeVisualKind::SecretTextFile { active }
    } else {
        NodeVisualKind::TextFile { active }
    };
    let style = node_label_style(tier, kind);

    prefix_spans.push(Span::styled(format!("  {checkbox}"), style));
    let mut right_spans = vec![Span::styled(
        format!(" ({})", format_token_count(file.tokens)),
        style,
    )];
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

fn discarded_row_parts(
    mut prefix_spans: Vec<Span<'static>>,
    clean_name: String,
    reason: FileDiscardReason,
) -> RowParts {
    let style = node_label_style(WeightTier::Light, NodeVisualKind::Discarded);
    prefix_spans.push(Span::styled(format!("  [{}] ", discard_tag(reason)), style));

    RowParts {
        prefix_spans,
        label: clean_name,
        label_style: style,
        right_spans: Vec::new(),
    }
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
