//! HTML escape / unescape helpers (independent of the template engine).

/// HTML-escape `&`, `<`, `>`, `"`, and `'` for element text / attributes.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// HTML-escape suitable for double-quoted attribute values (same as [`escape`]).
pub fn escape_attr(s: &str) -> String {
    escape(s)
}

/// Decode common HTML entities produced by [`escape`] plus numeric character references.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        if rest.starts_with('&') {
            if let Some((ch, n)) = decode_entity(rest) {
                out.push(ch);
                rest = &rest[n..];
                continue;
            }
        }
        let mut chars = rest.chars();
        if let Some(ch) = chars.next() {
            out.push(ch);
            rest = chars.as_str();
        } else {
            break;
        }
    }
    out
}

fn decode_entity(s: &str) -> Option<(char, usize)> {
    const NAMED: &[(&str, char)] = &[
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&#39;", '\''),
        ("&apos;", '\''),
    ];
    for &(ent, ch) in NAMED {
        if s.starts_with(ent) {
            return Some((ch, ent.len()));
        }
    }
    let rest = s.strip_prefix("&#")?;
    let end = rest.find(';')?;
    let body = &rest[..end];
    let cp = if let Some(hex) = body.strip_prefix('x').or_else(|| body.strip_prefix('X')) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        body.parse::<u32>().ok()?
    };
    let ch = char::from_u32(cp)?;
    Some((ch, 2 + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_basic() {
        let s = r#"a <b> & "c" 'd'"#;
        let e = escape(s);
        assert!(e.contains("&lt;"));
        assert_eq!(unescape(&e), s);
    }

    #[test]
    fn numeric_entity() {
        assert_eq!(unescape("&#65;"), "A");
        assert_eq!(unescape("&#x41;"), "A");
    }
}
