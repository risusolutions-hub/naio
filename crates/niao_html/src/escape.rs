//! HTML entity escape and unescape.

/// Escape text for HTML body context (`<`, `>`, `&`, `"`).
pub fn escape(text: &str) -> String {
    v_htmlescape::escape(text).to_string()
}

/// Escape text for HTML attribute values (also escapes `'`).
pub fn escape_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Unescape HTML entities (`&amp;`, `&#39;`, `&#x2F;`, …).
pub fn unescape(text: &str) -> String {
    html_escape::decode_html_entities(text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_roundtrip_body() {
        let raw = "a < b & \"c\"";
        assert_eq!(unescape(&escape(raw)), raw);
    }

    #[test]
    fn escape_attr_quotes() {
        assert!(escape_attr("it's").contains("&#39;"));
    }

    #[test]
    fn unescape_numeric() {
        assert_eq!(unescape("&#65;"), "A");
        assert_eq!(unescape("&#x41;"), "A");
    }
}
