//! Fast HTML entity escaping.

/// Escape `&`, `<`, `>`, `"`, `'` for HTML text nodes.
pub fn escape_html(s: &str) -> String {
    v_htmlescape::escape(s).to_string()
}

/// Escape for use inside a double-quoted attribute.
pub fn escape_attr(s: &str) -> String {
    // v_htmlescape::escape also escapes `"` and `'` — correct for attributes.
    v_htmlescape::escape(s).to_string()
}
