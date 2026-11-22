//! Strict XML 1.0 text and attribute escaping with illegal control-character sanitization.

pub(crate) fn push_escaped_xml_text(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#xD;"),
            _ if is_valid_xml10_char(ch) => out.push(ch),
            _ => out.push('\u{FFFD}'),
        }
    }
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
