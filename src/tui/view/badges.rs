//! Shared status tags, checkboxes, and secret spans reused across tree and inspector rows.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::domain::{FileDiscardReason, UnselectedReason};
use crate::tui::view::theme::secret_marker_style;

pub(super) fn checkbox_span(active: bool) -> Span<'static> {
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

pub(super) fn secret_span(active: bool) -> Span<'static> {
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

pub(super) const fn unselected_tag(reason: UnselectedReason) -> &'static str {
    match reason {
        UnselectedReason::UserExcluded => "off",
        UnselectedReason::DotIgnore => ".ignore",
        UnselectedReason::GitIgnore => ".gitignore",
        UnselectedReason::ExceedsTokenThreshold => ">50k",
        UnselectedReason::PrunedDir => "pruned",
    }
}

pub(super) const fn unselected_badge_style(reason: UnselectedReason) -> Style {
    match reason {
        UnselectedReason::DotIgnore => Style::new().fg(Color::Rgb(245, 210, 75)),
        UnselectedReason::GitIgnore => Style::new().fg(Color::Rgb(185, 165, 90)),
        UnselectedReason::ExceedsTokenThreshold => Style::new().fg(Color::Rgb(255, 95, 95)),
        UnselectedReason::UserExcluded | UnselectedReason::PrunedDir => {
            Style::new().fg(Color::Rgb(150, 155, 165))
        }
    }
}

pub(super) const fn discard_tag(reason: FileDiscardReason) -> &'static str {
    match reason {
        FileDiscardReason::Binary => "BIN",
        FileDiscardReason::InvalidUtf8 => "UTF8",
        FileDiscardReason::ExceedsHardCap { .. } => ">5MB",
        FileDiscardReason::Unreadable => "ERR",
    }
}

pub(super) const fn discard_badge_style(reason: FileDiscardReason) -> Style {
    match reason {
        FileDiscardReason::Binary => Style::new().fg(Color::Rgb(135, 145, 160)),
        FileDiscardReason::InvalidUtf8 => Style::new().fg(Color::Rgb(210, 120, 120)),
        FileDiscardReason::ExceedsHardCap { .. } => Style::new().fg(Color::Rgb(230, 140, 80)),
        FileDiscardReason::Unreadable => Style::new().fg(Color::Rgb(230, 90, 90)),
    }
}
