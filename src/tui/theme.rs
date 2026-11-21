//! Color palette encoding token weight by hue, secret files in purple, and state by brightness.

use ratatui::style::{Color, Modifier, Style};

use crate::domain::{SecretRuleKind, WeightTier};

/// Typed visual category preventing boolean parameter confusion when styling tree rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeVisualKind {
    /// Directory row rendered bold when active or dimmed when fully unselected.
    Directory {
        /// Whether at least one descendant file is currently selected.
        active: bool,
    },
    /// Readable `UTF-8` file row rendered bright when selected or dimmed when unselected.
    TextFile {
        /// Whether the file will be emitted to `<files>`.
        active: bool,
    },
    /// Text file carrying potential secret hits rendered in vivid or muted purple.
    SecretTextFile {
        /// Whether the file will be emitted to `<files>`.
        active: bool,
    },
    /// Non-text or oversized file rendered in muted slate gray.
    Discarded,
}

/// Returns the `ratatui` style for a tree node combining weight hue, secrets, and brightness.
#[must_use]
pub const fn node_label_style(tier: WeightTier, kind: NodeVisualKind) -> Style {
    let (is_active, is_directory) = match kind {
        NodeVisualKind::Discarded => {
            return Style::new()
                .fg(Color::Rgb(110, 115, 125))
                .add_modifier(Modifier::DIM);
        }
        NodeVisualKind::SecretTextFile { active } => {
            let fg = if active {
                Color::Rgb(205, 130, 255)
            } else {
                Color::Rgb(115, 72, 145)
            };
            let style = Style::new().fg(fg);
            return if active {
                style
            } else {
                style.add_modifier(Modifier::DIM)
            };
        }
        NodeVisualKind::Directory { active } => (active, true),
        NodeVisualKind::TextFile { active } => (active, false),
    };

    let fg = match (tier, is_active) {
        (WeightTier::Light, true) => Color::Rgb(85, 220, 125),
        (WeightTier::Light, false) => Color::Rgb(42, 102, 60),
        (WeightTier::Medium, true) => Color::Rgb(245, 210, 75),
        (WeightTier::Medium, false) => Color::Rgb(118, 100, 40),
        (WeightTier::Heavy, true) => Color::Rgb(255, 145, 55),
        (WeightTier::Heavy, false) => Color::Rgb(128, 74, 32),
        (WeightTier::Critical, true) => Color::Rgb(255, 85, 85),
        (WeightTier::Critical, false) => Color::Rgb(130, 46, 46),
    };

    let mut style = Style::new().fg(fg);
    if is_active && is_directory {
        style = style.add_modifier(Modifier::BOLD);
    } else if !is_active {
        style = style.add_modifier(Modifier::DIM);
    }
    style
}

/// Returns the highlight style applied to the active cursor row without erasing foreground hues.
#[must_use]
pub const fn cursor_row_style() -> Style {
    Style::new().bg(Color::Rgb(42, 52, 74))
}

/// Returns a high-contrast accent style for the active row cursor glyph (`▶`).
#[must_use]
pub const fn cursor_glyph_style() -> Style {
    Style::new()
        .fg(Color::Rgb(110, 210, 255))
        .add_modifier(Modifier::BOLD)
}

/// Returns the muted guide-line style used for tree hierarchy branches.
#[must_use]
pub const fn tree_guide_style() -> Style {
    Style::new().fg(Color::Rgb(75, 82, 98))
}

/// Returns the bright warning style for `[!]` secret indicators.
#[must_use]
pub const fn secret_marker_style() -> Style {
    Style::new()
        .fg(Color::Rgb(255, 90, 130))
        .add_modifier(Modifier::BOLD)
}

/// Returns the status-bar style for the global selected token counter.
#[must_use]
pub const fn total_tokens_style() -> Style {
    Style::new()
        .fg(Color::Rgb(110, 210, 255))
        .add_modifier(Modifier::BOLD)
}

/// Returns a concise display label for a matched [`SecretRuleKind`].
#[must_use]
pub const fn secret_rule_label(rule: SecretRuleKind) -> &'static str {
    match rule {
        SecretRuleKind::PrivateKey => "PrivateKey",
        SecretRuleKind::AwsAccessKey => "AwsAccessKey",
        SecretRuleKind::GitHubToken => "GitHubToken",
        SecretRuleKind::GoogleApiKey => "GoogleApiKey",
        SecretRuleKind::SlackToken => "SlackToken",
        SecretRuleKind::StripeLiveKey => "StripeLiveKey",
        SecretRuleKind::AiProviderKey => "AiProviderKey",
        SecretRuleKind::CredentialUrl => "CredentialUrl",
        SecretRuleKind::JwtToken => "JwtToken",
        SecretRuleKind::HardcodedSecret => "HardcodedSecret",
    }
}

/// Formats token counts compactly using integer math (e.g. `420`, `12.4k`, `1.2M`).
#[must_use]
pub fn format_token_count(tokens: usize) -> String {
    if tokens < 1_000 {
        return tokens.to_string();
    }
    if tokens < 1_000_000 {
        let whole = tokens / 1_000;
        let tenths = (tokens % 1_000) / 100;
        return format!("{whole}.{tenths}k");
    }
    let millions = tokens / 1_000_000;
    let tenths = (tokens % 1_000_000) / 100_000;
    format!("{millions}.{tenths}M")
}

/// Strips control characters and Unicode Bidi overrides from filenames before terminal rendering.
#[must_use]
pub fn sanitize_tui_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch.is_control() || is_bidi_override(ch) {
                '\u{FFFD}'
            } else {
                ch
            }
        })
        .collect()
}

const fn is_bidi_override(ch: char) -> bool {
    matches!(
        ch,
        '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}
