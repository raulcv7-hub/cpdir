//! Tab bar rendering, dynamic hit-box computation, and window slicing for the inspector modal.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::InspectorTab;

pub(super) fn render_tab_header(
    frame: &mut Frame<'_>,
    area: Rect,
    active: InspectorTab,
    unread: usize,
    discarded: usize,
    secrets: usize,
) -> [Rect; 3] {
    let tabs = [
        (
            InspectorTab::Unread,
            format!(" Unread ({unread}) "),
            Color::Rgb(245, 210, 75),
        ),
        (
            InspectorTab::Discarded,
            format!(" Discarded ({discarded}) "),
            Color::Rgb(150, 165, 185),
        ),
        (
            InspectorTab::Secrets,
            format!(" Secrets ({secrets}) "),
            Color::Rgb(205, 130, 255),
        ),
    ];

    let max_x = area.x.saturating_add(area.width);
    let mut x = area.x.saturating_add(1);
    let mut tab_rects = [Rect::default(); 3];
    let mut spans = vec![Span::raw(" ")];

    for (idx, (tab, label, color)) in tabs.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" | ", Style::new().fg(Color::Rgb(75, 82, 98))));
            x = x.saturating_add(3);
        }
        let label_w = u16::try_from(Span::raw(label.as_str()).width()).unwrap_or(u16::MAX);
        let clipped_w = label_w.min(max_x.saturating_sub(x));
        tab_rects[idx] = Rect::new(x, area.y, clipped_w, 1);
        x = x.saturating_add(label_w);

        let style = if tab == active {
            Style::new()
                .fg(Color::Rgb(20, 24, 32))
                .bg(color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(color)
        };
        spans.push(Span::styled(label, style));
    }

    let sep = "─".repeat(usize::from(area.width));
    let widget = Paragraph::new(vec![
        Line::from(spans),
        Line::styled(sep, Style::new().fg(Color::Rgb(75, 82, 98))),
    ]);
    frame.render_widget(widget, area);
    tab_rects
}

pub(super) fn compute_visible_window(
    cursor: usize,
    scroll: usize,
    total: usize,
    height: usize,
) -> (usize, usize) {
    if total == 0 || height == 0 {
        return (0, 0);
    }
    let max_start = total.saturating_sub(height);
    let mut start = scroll.min(max_start);
    if cursor < start {
        start = cursor;
    } else if cursor >= start + height {
        start = cursor + 1 - height;
    }
    (start, start.saturating_add(height).min(total))
}

pub(super) const fn tab_name(tab: InspectorTab) -> &'static str {
    match tab {
        InspectorTab::Unread => "Unread",
        InspectorTab::Discarded => "Discarded",
        InspectorTab::Secrets => "Secrets",
    }
}
