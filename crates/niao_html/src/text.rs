//! Text extraction and fast tag stripping.

use crate::error::{HtmlError, HtmlResult};
use crate::parse::{element_from_packed, unpack_node, DocumentStore};
use crate::select::parse_selector;
use scraper::{ElementRef, Html};

/// Options for bulk text extraction.
#[derive(Debug, Clone, Default)]
pub struct TextOpts {
    pub strip: bool,
    pub separator: String,
}

/// Strip HTML tags with a single-pass scanner (no full DOM). Leaves entity refs intact.
pub fn strip_tags(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    let mut in_tag = false;
    while i < bytes.len() {
        match bytes[i] {
            b'<' if !in_tag => {
                in_tag = true;
                i += 1;
            }
            b'>' if in_tag => {
                in_tag = false;
                i += 1;
            }
            _ if !in_tag => {
                out.push(bytes[i] as char);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Parse HTML and return all text (optionally filtered by CSS selector).
pub fn extract_text(html: &str, selector: Option<&str>, opts: &TextOpts) -> HtmlResult<String> {
    let doc = Html::parse_document(html);
    if let Some(sel) = selector {
        let selector = parse_selector(sel)?;
        let parts: Vec<String> = doc
            .select(&selector)
            .map(|el| collect_element_text(el, opts))
            .collect();
        Ok(join_parts(&parts, opts))
    } else {
        Ok(collect_element_text(doc.root_element(), opts))
    }
}

pub fn node_text(store: &DocumentStore, packed: i64, opts: &TextOpts) -> HtmlResult<String> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(collect_element_text(el, opts))
}

pub fn node_direct_text(store: &DocumentStore, packed: i64) -> HtmlResult<String> {
    let (doc_id, index) = unpack_node(packed)?;
    let doc = store
        .get(doc_id)
        .ok_or_else(|| HtmlError::InvalidHandle(format!("invalid document handle {doc_id}")))?;
    let node = doc.node_at(index)?;
    let mut out = String::new();
    for child in node.children() {
        if let scraper::node::Node::Text(t) = child.value() {
            out.push_str(t);
        }
    }
    Ok(out)
}

fn collect_element_text(el: ElementRef<'_>, opts: &TextOpts) -> String {
    if opts.separator.is_empty() && !opts.strip {
        return el.text().collect();
    }
    let parts: Vec<String> = el
        .text()
        .map(|s| {
            if opts.strip {
                s.split_whitespace().collect::<Vec<_>>().join(" ")
            } else {
                s.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect();
    join_parts(&parts, opts)
}

fn join_parts(parts: &[String], opts: &TextOpts) -> String {
    if opts.separator.is_empty() {
        parts.join("")
    } else {
        parts.join(&opts.separator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tags_basic() {
        assert_eq!(strip_tags("<p>hi</p>"), "hi");
        assert_eq!(strip_tags("a<b>c</b>d"), "acd");
    }

    #[test]
    fn extract_text_strip() {
        let opts = TextOpts {
            strip: true,
            separator: " ".into(),
        };
        let t = extract_text("<div>  hello   <span>world</span> </div>", None, &opts).unwrap();
        assert!(t.contains("hello"));
        assert!(t.contains("world"));
    }
}
