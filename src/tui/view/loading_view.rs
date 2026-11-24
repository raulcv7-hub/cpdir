//! Non-blocking startup progress view rendered before directory analysis completes.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::state::{LoadingPhase, LoadingProgress};
use crate::tui::view::theme::{format_token_count, sanitize_tui_label};

/// Renders live scan, tokenization, and Git progress without blocking the terminal screen.
pub fn render_loading_view(frame: &mut Frame<'_>, area: Rect, progress: &LoadingProgress) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .split(area);

    let root_name = sanitize_tui_label(&progress.root_name);
    let header = Paragraph::new(Line::from(vec![
        Span::raw(" Scanning workspace "),
        Span::styled(
            root_name,
            Style::new()
                .fg(Color::Rgb(110, 210, 255))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ..."),
    ]))
    .block(Block::default().borders(Borders::ALL).title(" cpdir "));
    frame.render_widget(header, chunks[0]);

    let phase_label = match progress.phase {
        LoadingPhase::Discovering => "1/3 Discovering directory tree & ignore rules",
        LoadingPhase::Analyzing => "2/3 Probing UTF-8, scanning secrets & counting tokens",
        LoadingPhase::Finalizing => "3/3 Applying workspace memory & finalizing view",
    };

    let git_label = if progress.git_pending {
        "running in background"
    } else {
        "ready"
    };

    let body_lines = vec![
        Line::raw(""),
        Line::styled(
            format!("  Stage:      {phase_label}"),
            Style::new()
                .fg(Color::Rgb(245, 210, 75))
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!(
            "  Folders:    {} visited  |  {} pruned",
            progress.dirs_scanned, progress.pruned_dirs
        )),
        Line::raw(format!(
            "  Files:      {}/{} analyzed  |  {} discarded",
            progress.analyzed_files, progress.candidate_files, progress.discarded_files
        )),
        Line::raw(format!(
            "  Tokens:     {} selected",
            format_token_count(progress.selected_tokens)
        )),
        Line::raw(format!("  Git meta:   {git_label}")),
    ];

    let body =
        Paragraph::new(body_lines).block(Block::default().borders(Borders::ALL).title(" Loading "));
    frame.render_widget(body, chunks[1]);

    let footer = Paragraph::new(Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "[Esc / q / Ctrl+C]",
            Style::new()
                .fg(Color::Rgb(255, 115, 115))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " Cancel startup",
            Style::new().fg(Color::Rgb(195, 202, 214)),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title(" Keys "));
    frame.render_widget(footer, chunks[2]);
}
