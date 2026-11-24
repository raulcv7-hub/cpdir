//! Adaptive header counters and color-coded footer key legend for the main tree view.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::domain::CategoryCounts;
use crate::tui::state::TuiAppState;
use crate::tui::view::theme::{
    format_token_count, sanitize_tui_label, secret_marker_style, total_tokens_style,
};

pub(super) fn render_header(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let counts = state.metrics().counts;
    let inner_w = usize::from(area.width.saturating_sub(2));
    let compact_hdr = inner_w < 68;
    let sep = if compact_hdr { " | " } else { "  |  " };
    let total_str = format_token_count(state.total_effective_tokens());

    let mut top_spans = vec![
        Span::raw(if compact_hdr { " Tok: " } else { " Tokens: " }),
        Span::styled(total_str, total_tokens_style()),
    ];

    if area.height < 4 {
        top_spans.push(Span::raw(sep));
        top_spans.extend(compact_count_spans(counts, inner_w >= 86));
    }

    if counts.active_secret_files > 0 {
        top_spans.push(Span::raw(sep));
        let lbl = if compact_hdr {
            format!("!Sec:{}", counts.active_secret_files)
        } else {
            format!("! Secrets: {}", counts.active_secret_files)
        };
        top_spans.push(Span::styled(lbl, secret_marker_style()));
    }
    if state.has_git_context() || state.is_git_pending() {
        top_spans.push(Span::raw(sep));
        top_spans.push(git_status_span(state, compact_hdr));
    }
    top_spans.push(Span::raw(sep));
    top_spans.push(path_status_span(state.include_abs_path(), compact_hdr));

    let mut lines = vec![Line::from(top_spans)];
    if area.height >= 4 {
        lines.push(Line::from(compact_count_spans(counts, inner_w >= 38)));
    }

    let root_name = sanitize_tui_label(&state.tree().nodes()[0].name);
    let header = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" cpdir - {root_name} ")),
    );
    frame.render_widget(header, area);
}

pub(super) fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let inner_w = usize::from(area.width.saturating_sub(2));
    let wide = inner_w >= 110;
    let medium = inner_w >= 86;
    let compact = inner_w >= 68;
    let sep = if wide { "  " } else { " " };

    let mut hints = vec![
        (
            "[Enter]",
            if compact { " Copy" } else { "" },
            Color::Rgb(85, 220, 125),
        ),
        (
            "[Space]",
            if wide {
                " Toggle"
            } else if medium {
                " Sel"
            } else {
                ""
            },
            Color::Rgb(110, 210, 255),
        ),
        (
            "[Tab]",
            if medium { " Fold" } else { "" },
            Color::Rgb(245, 210, 75),
        ),
        (
            "[a]",
            if wide {
                " All/None"
            } else if medium {
                " All"
            } else {
                ""
            },
            Color::Rgb(245, 210, 75),
        ),
        (
            "[i]",
            if wide {
                " Inspect"
            } else if medium {
                " Insp"
            } else {
                ""
            },
            Color::Rgb(140, 185, 255),
        ),
    ];

    if state.has_git_context() {
        hints.push((
            "[g]",
            if compact { " Git" } else { "" },
            toggle_hint_color(state.include_git_context()),
        ));
    }
    hints.push((
        "[p]",
        if compact { " Path" } else { "" },
        toggle_hint_color(state.include_abs_path()),
    ));
    hints.push((
        "[?]",
        if wide { " Help" } else { "" },
        Color::Rgb(205, 130, 255),
    ));
    hints.push((
        "[Esc]",
        if compact { " Quit" } else { "" },
        Color::Rgb(255, 115, 115),
    ));

    let mut spans = vec![Span::raw(" ")];
    for (idx, (key, label, fg)) in hints.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(sep));
        }
        spans.push(Span::styled(
            key,
            Style::new().fg(fg).add_modifier(Modifier::BOLD),
        ));
        if !label.is_empty() {
            spans.push(Span::styled(
                label,
                Style::new().fg(Color::Rgb(195, 202, 214)),
            ));
        }
    }

    let footer = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(" Keys "));
    frame.render_widget(footer, area);
}

fn compact_count_spans(counts: CategoryCounts, verbose: bool) -> Vec<Span<'static>> {
    let (r_lbl, u_lbl, d_lbl) = if verbose {
        (" Read:", "  Unread:", "  Discarded:")
    } else {
        (" R:", "  U:", "  D:")
    };
    vec![
        Span::styled(
            format!("{r_lbl} {}", counts.read_files),
            Style::new().fg(Color::Rgb(85, 220, 125)),
        ),
        Span::styled(
            format!("{u_lbl} {}", counts.unread_files),
            Style::new().fg(Color::Rgb(245, 210, 75)),
        ),
        Span::styled(
            format!("{d_lbl} {}", counts.discarded_items),
            Style::new().fg(Color::Rgb(140, 145, 155)),
        ),
    ]
}

fn git_status_span(state: &TuiAppState, compact: bool) -> Span<'static> {
    if state.is_git_pending() && !state.has_git_context() {
        return Span::styled(
            if compact { "G:..." } else { "Git: ..." },
            Style::new().fg(Color::Rgb(245, 210, 75)),
        );
    }
    if state.include_git_context() {
        let git_tokens = state.git_context_tokens();
        let label = match (compact, git_tokens > 0) {
            (true, true) => format!("G:+{}", format_token_count(git_tokens)),
            (true, false) => "G:ON".to_owned(),
            (false, true) => format!("Git: ON (+{})", format_token_count(git_tokens)),
            (false, false) => "Git: ON".to_owned(),
        };
        Span::styled(label, Style::new().fg(Color::Rgb(85, 220, 125)))
    } else {
        Span::styled(
            if compact { "G:OFF" } else { "Git: OFF" },
            Style::new()
                .fg(Color::Rgb(120, 125, 135))
                .add_modifier(Modifier::DIM),
        )
    }
}

fn path_status_span(enabled: bool, compact: bool) -> Span<'static> {
    if enabled {
        Span::styled(
            if compact { "P:ON" } else { "Path: ON" },
            Style::new().fg(Color::Rgb(85, 220, 125)),
        )
    } else {
        Span::styled(
            if compact { "P:OFF" } else { "Path: OFF" },
            Style::new()
                .fg(Color::Rgb(120, 125, 135))
                .add_modifier(Modifier::DIM),
        )
    }
}

const fn toggle_hint_color(enabled: bool) -> Color {
    if enabled {
        Color::Rgb(85, 220, 125)
    } else {
        Color::Rgb(130, 135, 145)
    }
}
