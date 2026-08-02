//! DOM parsing via quick-xml.

use crate::dom::{Attr, Document, Element, Node, XmlOpts};
use crate::error::{XmlError, MAX_BYTES, MAX_NODES};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

#[derive(Clone)]
struct NsScope {
    prefixes: HashMap<Vec<u8>, String>,
}

impl NsScope {
    fn new() -> Self {
        Self {
            prefixes: HashMap::new(),
        }
    }

    fn inherit_from(parent: &NsScope) -> Self {
        Self {
            prefixes: parent.prefixes.clone(),
        }
    }

    fn apply_start(&mut self, e: &quick_xml::events::BytesStart<'_>) {
        for attr in e.attributes().with_checks(false).flatten() {
            let key = attr.key.as_ref();
            if key.starts_with(b"xmlns") {
                let uri = String::from_utf8_lossy(&attr.value).into_owned();
                if key.len() == 5 {
                    self.prefixes.insert(Vec::new(), uri);
                } else if key.len() > 6 && key[5] == b':' {
                    self.prefixes.insert(key[6..].to_vec(), uri);
                }
            }
        }
    }

    fn tag_parts(&self, raw: &[u8]) -> (Option<String>, Option<String>, String) {
        match raw.iter().position(|&b| b == b':') {
            Some(i) => {
                let prefix = String::from_utf8_lossy(&raw[..i]).into_owned();
                let local = String::from_utf8_lossy(&raw[i + 1..]).into_owned();
                let ns = self.prefixes.get(&raw[..i]).cloned();
                (Some(prefix), ns, local)
            }
            None => {
                let local = String::from_utf8_lossy(raw).into_owned();
                let ns = self.prefixes.get(&[] as &[u8]).cloned();
                (None, ns, local)
            }
        }
    }
}

fn parse_attrs(e: &quick_xml::events::BytesStart<'_>, ns: &NsScope) -> Vec<Attr> {
    let mut out = Vec::new();
    for attr in e.attributes().with_checks(false).flatten() {
        let key = attr.key.as_ref();
        if key.starts_with(b"xmlns") {
            continue;
        }
        let (prefix, namespace, local) = ns.tag_parts(key);
        let value = String::from_utf8_lossy(&attr.value).into_owned();
        out.push(Attr {
            local,
            prefix,
            namespace,
            value,
        });
    }
    out
}

fn decl_attr(bytes: std::borrow::Cow<'_, [u8]>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

fn push_start(
    e: &quick_xml::events::BytesStart<'_>,
    stack: &mut Vec<Element>,
    ns_stack: &mut Vec<NsScope>,
    node_count: &mut usize,
    huge_tree: bool,
) -> Result<(), XmlError> {
    *node_count += 1;
    if !huge_tree && *node_count > MAX_NODES {
        return Err(XmlError::TooManyNodes);
    }
    let mut scope = NsScope::inherit_from(ns_stack.last().unwrap());
    scope.apply_start(e);
    let (prefix, namespace, local) = scope.tag_parts(e.name().as_ref());
    let el = Element {
        tag: local,
        prefix,
        namespace,
        attrs: parse_attrs(e, &scope),
        text: String::new(),
        tail: String::new(),
        children: Vec::new(),
    };
    ns_stack.push(scope);
    stack.push(el);
    Ok(())
}

fn push_text(stack: &mut Vec<Element>, text: String, node_count: &mut usize) {
    if text.is_empty() {
        return;
    }
    *node_count += 1;
    if let Some(top) = stack.last_mut() {
        if top.text.is_empty() && top.children.iter().all(|c| !matches!(c, Node::Text(_))) {
            top.text.push_str(&text);
        } else {
            top.children.push(Node::Text(text));
        }
    }
}

fn pop_element(stack: &mut Vec<Element>, doc: &mut Document) -> Result<(), XmlError> {
    let finished = stack
        .pop()
        .ok_or_else(|| XmlError::parse(0, 0, "unexpected end tag"))?;
    if let Some(parent) = stack.last_mut() {
        parent.children.push(Node::Element(finished));
    } else {
        doc.root = Some(finished);
    }
    Ok(())
}

/// Parse XML string into a DOM document.
pub fn parse(input: &str, opts: &XmlOpts) -> Result<Document, XmlError> {
    if input.len() > MAX_BYTES {
        return Err(XmlError::TooLarge(input.len()));
    }
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = true;

    let mut buf = Vec::new();
    let mut stack: Vec<Element> = Vec::new();
    let mut ns_stack: Vec<NsScope> = vec![NsScope::new()];
    let mut doc = Document::empty();
    let mut node_count = 0usize;

    loop {
        let pos = reader.buffer_position() as u32;
        match reader.read_event_into(&mut buf) {
            Ok(Event::Decl(e)) => {
                if let Ok(v) = e.version() {
                    doc.version = Some(decl_attr(v));
                }
                if let Some(Ok(enc)) = e.encoding() {
                    doc.encoding = Some(decl_attr(enc));
                }
            }
            Ok(Event::DocType(_)) => {}
            Ok(Event::PI(_)) if !opts.keep_pi => {}
            Ok(Event::Comment(e)) if opts.keep_comments => {
                node_count += 1;
                let text = e
                    .unescape()
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()).into_owned());
                if let Some(top) = stack.last_mut() {
                    top.children.push(Node::Comment(text));
                }
            }
            Ok(Event::Comment(_)) => {}
            Ok(Event::Start(e)) => {
                push_start(
                    &e,
                    &mut stack,
                    &mut ns_stack,
                    &mut node_count,
                    opts.huge_tree,
                )?;
            }
            Ok(Event::Empty(e)) => {
                push_start(
                    &e,
                    &mut stack,
                    &mut ns_stack,
                    &mut node_count,
                    opts.huge_tree,
                )?;
                ns_stack.pop();
                pop_element(&mut stack, &mut doc)?;
            }
            Ok(Event::End(_)) => {
                ns_stack.pop();
                pop_element(&mut stack, &mut doc)?;
            }
            Ok(Event::Text(e)) => {
                let text = e
                    .unescape()
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()).into_owned());
                push_text(&mut stack, text, &mut node_count);
            }
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).into_owned();
                push_text(&mut stack, text, &mut node_count);
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                if opts.recover {
                    let _ = pos;
                    buf.clear();
                    continue;
                }
                return Err(XmlError::parse(0, pos, e.to_string()));
            }
            _ => {}
        }
        buf.clear();
    }

    if doc.root.is_none() {
        return Err(XmlError::parse(1, 1, "no root element"));
    }
    doc.check_limits(opts.huge_tree)?;
    Ok(doc)
}

/// Parse XML bytes (UTF-8).
pub fn parse_bytes(input: &[u8], opts: &XmlOpts) -> Result<Document, XmlError> {
    let s = std::str::from_utf8(input).map_err(|e| XmlError::Parse {
        line: 1,
        col: 1,
        message: format!("invalid UTF-8: {e}"),
    })?;
    parse(s, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let doc = parse("<root><a>1</a></root>", &XmlOpts::default()).unwrap();
        let root = doc.root.as_ref().unwrap();
        assert_eq!(root.tag, "root");
        assert_eq!(root.child_elements().len(), 1);
        assert_eq!(root.child_elements()[0].text, "1");
    }

    #[test]
    fn parse_attrs() {
        let doc = parse(r#"<root id="x"/>"#, &XmlOpts::default()).unwrap();
        assert_eq!(doc.root.as_ref().unwrap().get_attr("id"), Some("x"));
    }
}
