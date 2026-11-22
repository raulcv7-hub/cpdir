//! Quick-reference keybindings and color-tier legend modal overlay (`?`).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tui::state::TuiAppState;
use crate::tui::view::confirm_modals::centered_fixed_rect;

pub(super) fn render_help_modal(frame: &mut Frame<'_>, area: Rect, state: &TuiAppState) {
    let popup = centered_fixed_rect(76, 56, 17, area);
    frame.render_widget(Clear, popup);

    let git_line = if state.has_git_context() {
        "  g              Toggle <git_context> inclusion"
    } else {
        "  g              Git context unavailable outside repositories"
    };

    let lines = vec![
        Line::raw(""),
        help_row("Navigation", Color::Rgb(110, 210, 255)),
        Line::raw("  Up/Down, j/k   Move cursor | PgUp/PgDn, Home/End"),
        Line::raw("  Tab / Left/Right  Fold/unfold folder or jump to parent"),
        Line::raw("  c / e          Collapse all subfolders / Expand all"),
        Line::raw(""),
        help_row("Selection & Inspector", Color::Rgb(85, 220, 125)),
        Line::raw("  Space          Toggle file or subtree recursively"),
        Line::raw("  a              Toggle All (safe defaults) / None"),
        Line::raw("  i / n / d / s  Inspector (Unread / Discarded / Secrets)"),
        Line::raw(git_line),
        Line::raw("  Enter / Esc    Copy strict XML to clipboard / Quit"),
        Line::raw(""),
        help_row(
            "Colors (Hue = Tokens/Secrets, Bright = Active)",
            Color::Rgb(245, 210, 75),
        ),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("<2k ", Style::new().fg(Color::Rgb(85, 220, 125))),
            Span::styled("<15k ", Style::new().fg(Color::Rgb(245, 210, 75))),
            Span::styled("<50k ", Style::new().fg(Color::Rgb(255, 145, 55))),
            Span::styled(">=50k ", Style::new().fg(Color::Rgb(255, 85, 85))),
            Span::styled("Secret[!]", Style::new().fg(Color::Rgb(205, 130, 255))),
        ]),
    ];

    let dialog = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::Rgb(110, 210, 255)))
            .title(" Help - Press Esc or ? to close ")
            .style(Style::new().bg(Color::Rgb(22, 26, 36))),
    );
    frame.render_widget(dialog, popup);
}

fn help_row(title: &'static str, fg: Color) -> Line<'static> {
    Line::styled(
        format!("  {title}"),
        Style::new().fg(fg).add_modifier(Modifier::BOLD),
    )
}
