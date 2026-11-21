//! Row formatting helpers for the unified Unread / Discarded / Secrets inspector modal.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::domain::{
    FileDiscardReason, NodeId, NodeKind, PrunedDirReason, SecretRuleKind, UnselectedReason,
};
use crate::tui::app_state::TuiAppState;
use crate::tui::inspector_items::{InspectorEntry, InspectorEntryKind};
use crate::tui::theme::{
    cursor_glyph_style, cursor_row_style, format_token_count, sanitize_tui_label,
    secret_marker_style, secret_rule_label,
};

pub(super) fn build_inspector_entry_line(
    state: &TuiAppState,
    entry: &InspectorEntry,
    is_cursor: bool,
) -> Line<'static> {
    let mut spans = vec![if is_cursor {
        Span::styled("▶ ", cursor_glyph_style())
    } else {
        Span::raw("  ")
    }];

    let clean_path = sanitize_tui_label(&entry.rel_path);
    match entry.kind {
        InspectorEntryKind::UnreadDir {
            node_id,
            unread_count,
            tokens,
        } => push_unread_dir_spans(
            &mut spans,
            state,
            node_id,
            &clean_path,
            unread_count,
            tokens,
        ),
        InspectorEntryKind::UnreadFile {
            node_id,
            reason,
            tokens,
        } => push_unread_file_spans(&mut spans, state, node_id, &clean_path, reason, tokens),
        InspectorEntryKind::DiscardedPrunedDir {
            reason: PrunedDirReason::DependencyOrVcs,
        } => push_pruned_dir_spans(&mut spans, &clean_path),
        InspectorEntryKind::DiscardedFile { reason, .. } => {
            push_discarded_file_spans(&mut spans, &clean_path, reason);
        }
        InspectorEntryKind::SecretItem {
            node_id,
            line,
            rule,
            hits,
        } => push_secret_spans(&mut spans, state, node_id, &clean_path, line, rule, hits),
    }

    let mut row = Line::from(spans);
    if is_cursor {
        row = row.style(cursor_row_style());
    }
    row
}

fn push_unread_dir_spans(
    spans: &mut Vec<Span<'static>>,
    state: &TuiAppState,
    node_id: NodeId,
    clean_path: &str,
    unread_count: usize,
    tokens: usize,
) {
    let active = state.metrics().rollups[node_id.index()].selected_files > 0;
    spans.push(checkbox_span(active));
    spans.push(Span::styled(
        "[DIR]",
        Style::new()
            .fg(Color::Rgb(110, 210, 255))
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(format!(
        " {clean_path}/ ({unread_count} files, {})",
        format_token_count(tokens)
    )));
}

fn push_unread_file_spans(
    spans: &mut Vec<Span<'static>>,
    state: &TuiAppState,
    node_id: NodeId,
    clean_path: &str,
    reason: UnselectedReason,
    tokens: usize,
) {
    let active = is_node_selected(state, node_id);
    let (tag, tag_style) = unselected_badge(reason);
    spans.push(checkbox_span(active));
    spans.push(Span::styled(format!("[{tag}]"), tag_style));
    spans.push(Span::raw(format!(" {clean_path}")));
    if tokens > 0 {
        spans.push(Span::styled(
            format!(" ({})", format_token_count(tokens)),
            Style::new()
                .fg(Color::Rgb(140, 145, 155))
                .add_modifier(Modifier::DIM),
        ));
    }
}

fn push_pruned_dir_spans(spans: &mut Vec<Span<'static>>, clean_path: &str) {
    spans.push(Span::styled(
        "[ ] ",
        Style::new().fg(Color::Rgb(110, 210, 255)),
    ));
    spans.push(Span::styled(
        "[PRUNED]",
        Style::new().fg(Color::Rgb(150, 155, 165)),
    ));
    spans.push(Span::raw(format!(" {clean_path}/")));
    spans.push(Span::styled(
        " [Space: load]",
        Style::new()
            .fg(Color::Rgb(110, 210, 255))
            .add_modifier(Modifier::DIM),
    ));
}

fn push_discarded_file_spans(
    spans: &mut Vec<Span<'static>>,
    clean_path: &str,
    reason: FileDiscardReason,
) {
    let (tag, tag_style) = discard_badge(reason);
    spans.push(Span::raw("    "));
    spans.push(Span::styled(format!("[{tag}]"), tag_style));
    spans.push(Span::raw(format!(" {clean_path}")));
    spans.push(Span::styled(
        " (non-text)",
        Style::new()
            .fg(Color::Rgb(120, 125, 135))
            .add_modifier(Modifier::DIM),
    ));
}

fn push_secret_spans(
    spans: &mut Vec<Span<'static>>,
    state: &TuiAppState,
    node_id: NodeId,
    clean_path: &str,
    line: Option<usize>,
    rule: Option<SecretRuleKind>,
    hits: usize,
) {
    let active = is_node_selected(state, node_id);
    spans.push(checkbox_span(active));
    let badge = rule.map_or("SECRET", secret_rule_label);
    spans.push(Span::styled(format!("[{badge}]"), secret_marker_style()));
    let loc = match line {
        Some(ln) => format!(" {clean_path}:{ln}"),
        None => format!(" {clean_path} ({hits} hit(s))"),
    };
    spans.push(Span::styled(
        loc,
        Style::new().fg(if active {
            Color::Rgb(205, 130, 255)
        } else {
            Color::Rgb(140, 110, 165)
        }),
    ));
}

fn is_node_selected(state: &TuiAppState, id: NodeId) -> bool {
    matches!(
        state.tree().get(id).map(|n| &n.kind),
        Some(NodeKind::TextFile(f)) if f.state.is_selected()
    )
}

fn checkbox_span(active: bool) -> Span<'static> {
    if active {
        Span::styled(
            "[x] ",
            Style::new()
                .fg(Color::Rgb(85, 220, 125))
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("[ ] ", Style::new().fg(Color::Rgb(130, 135, 145)))
    }
}

const fn unselected_badge(reason: UnselectedReason) -> (&'static str, Style) {
    match reason {
        UnselectedReason::DotIgnore => (".ignore", Style::new().fg(Color::Rgb(245, 210, 75))),
        UnselectedReason::GitIgnore => (".gitignore", Style::new().fg(Color::Rgb(185, 165, 90))),
        UnselectedReason::ExceedsTokenThreshold => {
            (">50k", Style::new().fg(Color::Rgb(255, 95, 95)))
        }
        UnselectedReason::UserExcluded => ("off", Style::new().fg(Color::Rgb(150, 155, 165))),
    }
}

const fn discard_badge(reason: FileDiscardReason) -> (&'static str, Style) {
    match reason {
        FileDiscardReason::Binary => ("BIN", Style::new().fg(Color::Rgb(135, 145, 160))),
        FileDiscardReason::InvalidUtf8 => ("UTF8", Style::new().fg(Color::Rgb(210, 120, 120))),
        FileDiscardReason::ExceedsHardCap { .. } => {
            (">5MB", Style::new().fg(Color::Rgb(230, 140, 80)))
        }
        FileDiscardReason::Unreadable => ("ERR", Style::new().fg(Color::Rgb(230, 90, 90))),
    }
}
