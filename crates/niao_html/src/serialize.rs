//! HTML serialization and prettify.

use crate::error::HtmlResult;
use crate::parse::element_from_packed;
use crate::parse::DocumentStore;

pub fn outer_html(store: &DocumentStore, packed: i64) -> HtmlResult<String> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el.html())
}

pub fn inner_html(store: &DocumentStore, packed: i64) -> HtmlResult<String> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el.inner_html())
}

/// Pretty-print HTML with indentation (best-effort serializer).
pub fn prettify(store: &DocumentStore, packed: i64, indent: usize) -> HtmlResult<String> {
    let html = outer_html(store, packed)?;
    Ok(pretty_format(&html, indent))
}

fn pretty_format(html: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::with_capacity(html.len() + html.len() / 4);
    let mut depth = 0usize;
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                depth = depth.saturating_sub(1);
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&pad.repeat(depth));
            } else if i + 1 < bytes.len() && bytes[i + 1] == b'!' {
                // comment/doctype — copy until >
            } else {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&pad.repeat(depth));
                depth += 1;
            }
            let start = i;
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            out.push_str(std::str::from_utf8(&bytes[start..i]).unwrap_or(""));
            if bytes.get(start + 1) == Some(&b'/') || is_void_tag(&bytes[start..i]) {
                depth = depth.saturating_sub(1);
            }
            continue;
        }
        // text run
        let start = i;
        while i < bytes.len() && bytes[i] != b'<' {
            i += 1;
        }
        let text = std::str::from_utf8(&bytes[start..i]).unwrap_or("").trim();
        if !text.is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&pad.repeat(depth));
            out.push_str(text);
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn is_void_tag(tag: &[u8]) -> bool {
    const VOID: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];
    let s = std::str::from_utf8(tag).unwrap_or("");
    let name = s
        .trim_start_matches('<')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches('>')
        .trim_end_matches('/');
    VOID.iter().any(|v| name.eq_ignore_ascii_case(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{alloc_document, root_node};
    use crate::select::select_one;

    #[test]
    fn inner_outer() {
        let mut store = DocumentStore::new();
        let id = alloc_document(&mut store, "<div><b>x</b></div>", false);
        let root = root_node(&store, id).unwrap();
        let div = select_one(&store, root, "div").unwrap().unwrap();
        assert!(inner_html(&store, div).unwrap().contains("<b>"));
        assert!(outer_html(&store, div).unwrap().starts_with("<div"));
    }
}
