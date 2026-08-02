//! Lightweight RFC 5322 header parsing (no full MIME tree).

use std::collections::BTreeMap;

/// Parse message headers from raw RFC822 bytes/text.
/// Returns lowercased header names → values (folded whitespace collapsed).
pub fn parse_headers(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let header_block = match raw.find("\r\n\r\n") {
        Some(i) => &raw[..i],
        None => match raw.find("\n\n") {
            Some(i) => &raw[..i],
            None => raw,
        },
    };

    let mut current_name: Option<String> = None;
    let mut current_value = String::new();

    for line in header_block.lines() {
        if line.is_empty() {
            break;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_name.is_some() {
                current_value.push(' ');
                current_value.push_str(line.trim());
            }
            continue;
        }
        if let Some(name) = current_name.take() {
            out.insert(name, current_value.clone());
            current_value.clear();
        }
        if let Some(colon) = line.find(':') {
            let name = line[..colon].trim().to_ascii_lowercase();
            let value = line[colon + 1..].trim().to_string();
            current_name = Some(name);
            current_value = value;
        }
    }
    if let Some(name) = current_name {
        out.insert(name, current_value);
    }
    out
}

/// Quote an IMAP atom/string (escape `\` and `"`).
pub fn imap_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Format message set from ints: `[1,2,5]` → `"1,2,5"`, empty → error string "".
pub fn format_message_set(ids: &[u32]) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<u32> = ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts: Vec<String> = Vec::new();
    let mut start = sorted[0];
    let mut prev = sorted[0];
    for &id in &sorted[1..] {
        if id == prev + 1 {
            prev = id;
            continue;
        }
        if start == prev {
            parts.push(start.to_string());
        } else {
            parts.push(format!("{start}:{prev}"));
        }
        start = id;
        prev = id;
    }
    if start == prev {
        parts.push(start.to_string());
    } else {
        parts.push(format!("{start}:{prev}"));
    }
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_basic() {
        let raw = "From: a@b.com\r\nSubject: Hello\r\n\r\nBody";
        let h = parse_headers(raw);
        assert_eq!(h.get("from").map(String::as_str), Some("a@b.com"));
        assert_eq!(h.get("subject").map(String::as_str), Some("Hello"));
    }

    #[test]
    fn headers_folded() {
        let raw = "Subject: hello\r\n world\r\n\r\n";
        let h = parse_headers(raw);
        assert_eq!(h.get("subject").map(String::as_str), Some("hello world"));
    }

    #[test]
    fn quote_escapes() {
        assert_eq!(imap_quote(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn message_set_ranges() {
        assert_eq!(format_message_set(&[1, 2, 3, 5, 7, 8]), "1:3,5,7:8");
        assert_eq!(format_message_set(&[]), "");
    }
}
