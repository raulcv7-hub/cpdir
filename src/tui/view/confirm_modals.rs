//! Confirmation dialogs guarding empty selection copy and active secret-bearing files.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::domain::{NodeId, NodeKind};
use crate::tui::state::TuiAppState;
use crate::tui::view::theme::{sanitize_tui_label, secret_marker_style, secret_rule_label};

const MAX_MODAL_SECRET_PATHS: usize = 5;

struct FlaggedSecretRow {
    rel_path: String,
    first_finding: Option<(usize, &'static str)>,
    total_hits: usize,
}

pub(super) fn render_confirm_secrets_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiAppState,
    active_secret_files: usize,
) {
    let flagged: Vec<FlaggedSecretRow> = state
        .tree()
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(idx, n)| match &n.kind {
            NodeKind::TextFile(f) if f.secret_hits > 0 && f.state.is_selected() => {
                let first_finding = state
                    .tree()
                    .secret_findings(NodeId(idx))
                    .first()
                    .map(|finding| (finding.line, secret_rule_label(finding.rule)));
                Some(FlaggedSecretRow {
                    rel_path: sanitize_tui_label(&n.rel_path),
                    first_finding,
                    total_hits: f.secret_hits,
                })
            }
            _ => None,
        })
        .collect();

    let shown = flagged.len().min(MAX_MODAL_SECRET_PATHS);
    let has_more = flagged.len() > MAX_MODAL_SECRET_PATHS;
    let extra_rows = u16::try_from(shown + usize::from(has_more)).unwrap_or(6);
    let popup = centered_fixed_rect(76, 58, 8 + extra_rows, area);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::raw(""),
        Line::styled(
            format!("  {active_secret_files} selected file(s) contain potential secrets [!]."),
            secret_marker_style(),
        ),
    ];
    for row in flagged.iter().take(MAX_MODAL_SECRET_PATHS) {
        let detail = match row.first_finding {
            Some((line, rule)) => format!(
                "    - {} ({} hit(s)) - line {line} [{rule}]",
                row.rel_path, row.total_hits
            ),
            None => format!("    - {} ({} hit(s))", row.rel_path, row.total_hits),
        };
        lines.push(Line::styled(
            detail,
            Style::new().fg(Color::Rgb(245, 210, 75)),
        ));
    }
    if has_more {
        let rest = flagged.len() - MAX_MODAL_SECRET_PATHS;
        lines.push(Line::raw(format!("    ... (+{rest} more flagged file(s))")));
    }
    lines.push(Line::raw(""));
    lines.push(secret_confirm_actions_line());

    let dialog = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(secret_marker_style())
            .title(" ! Potential Secrets Selected ")
            .style(Style::new().bg(Color::Rgb(30, 22, 28))),
    );
    frame.render_widget(dialog, popup);
}

pub(super) fn render_confirm_empty_modal(frame: &mut Frame<'_>, area: Rect) {
    let popup = centered_fixed_rect(62, 46, 8, area);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::raw(""),
        Line::styled(
            "  0 files are selected. Copy tree structure / git context only?",
            Style::new()
                .fg(Color::Rgb(245, 210, 75))
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        confirm_actions_line(),
    ];

    let dialog = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::Rgb(245, 210, 75)))
            .title(" Empty Selection ")
            .style(Style::new().bg(Color::Rgb(26, 30, 40))),
    );
    frame.render_widget(dialog, popup);
}

pub(super) fn centered_fixed_rect(pct_w: u16, min_w: u16, height: u16, area: Rect) -> Rect {
    let popup_w = (area.width.saturating_mul(pct_w) / 100)
        .max(min_w)
        .min(area.width);
    let popup_h = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(popup_w) / 2,
        y: area.y + area.height.saturating_sub(popup_h) / 2,
        width: popup_w,
        height: popup_h,
    }
}

fn secret_confirm_actions_line() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "[y/Enter]",
            Style::new()
                .fg(Color::Rgb(255, 145, 55))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Copy anyway   "),
        Span::styled(
            "[x]",
            Style::new()
                .fg(Color::Rgb(205, 130, 255))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Exclude flagged   "),
        Span::styled(
            "[n/Esc]",
            Style::new()
                .fg(Color::Rgb(85, 220, 125))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Return"),
    ])
}

fn confirm_actions_line() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "[y / Enter]",
            Style::new()
                .fg(Color::Rgb(255, 145, 55))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Copy anyway    "),
        Span::styled(
            "[n / Esc]",
            Style::new()
                .fg(Color::Rgb(85, 220, 125))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Return to tree"),
    ])
}
