//! Strict XML 1.0 escaping, `CDATA` wrapping, and control-character sanitization.

use std::iter::Peekable;
use std::str::Chars;

pub(crate) fn push_escaped_xml_text(out: &mut String, text: &str) {
    let mut chars = text.chars().peekable();
    while let Some(ch) = next_normalized_xml_char(&mut chars) {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

pub(crate) fn push_cdata_xml_body(out: &mut String, text: &str) {
    out.push_str("<![CDATA[");
    let mut brackets: u8 = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = next_normalized_xml_char(&mut chars) {
        match ch {
            ']' => {
                brackets = brackets.saturating_add(1).min(2);
                out.push(']');
            }
            '>' if brackets == 2 => {
                out.push_str("]]><![CDATA[>");
                brackets = 0;
            }
            _ => {
                brackets = 0;
                out.push(ch);
            }
        }
    }
    out.push_str("]]>");
}

pub(crate) fn push_escaped_xml_attr(out: &mut String, attr: &str) {
    for ch in attr.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\r' => out.push_str("&#xD;"),
            '\n' => out.push_str("&#xA;"),
            '\t' => out.push_str("&#x9;"),
            _ if is_valid_xml10_char(ch) => out.push(ch),
            _ => out.push('\u{FFFD}'),
        }
    }
}

fn next_normalized_xml_char(chars: &mut Peekable<Chars<'_>>) -> Option<char> {
    let ch = chars.next()?;
    Some(match ch {
        '\r' => {
            if chars.peek() == Some(&'\n') {
                let _ = chars.next();
            }
            '\n'
        }
        _ if is_valid_xml10_char(ch) => ch,
        _ => '\u{FFFD}',
    })
}

const fn is_valid_xml10_char(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n'
            | '\r'
            | '\u{0020}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
    )
}
