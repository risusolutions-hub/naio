//! Serialize DOM to XML.

use crate::dom::{Document, Element, Node, XmlOpts};
use crate::error::XmlError;
use quick_xml::events::{BytesDecl, BytesEnd, BytesPI, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::io::Cursor;

fn write_element(
    w: &mut Writer<Cursor<Vec<u8>>>,
    el: &Element,
    opts: &XmlOpts,
    depth: usize,
) -> Result<(), XmlError> {
    let indent = opts.indent.as_deref().unwrap_or("  ");

    if opts.pretty && depth > 0 {
        let pad = indent.repeat(depth);
        w.write_event(Event::Text(BytesText::new(&format!("\n{pad}"))))
            .map_err(|e| XmlError::Emit(e.to_string()))?;
    }

    let qname = el.qname();
    let mut start = BytesStart::new(qname.as_str());
    for a in &el.attrs {
        let key = a.key();
        start.push_attribute((key.as_str(), a.value.as_str()));
    }
    w.write_event(Event::Start(start))
        .map_err(|e| XmlError::Emit(e.to_string()))?;

    let has_element_children = el.children.iter().any(|c| matches!(c, Node::Element(_)));

    if !el.text.is_empty() {
        w.write_event(Event::Text(BytesText::new(&el.text)))
            .map_err(|e| XmlError::Emit(e.to_string()))?;
    }

    for child in &el.children {
        match child {
            Node::Element(c) => write_element(w, c, opts, depth + 1)?,
            Node::Text(t) => {
                w.write_event(Event::Text(BytesText::new(t)))
                    .map_err(|e| XmlError::Emit(e.to_string()))?;
            }
            Node::Comment(c) => {
                w.write_event(Event::Comment(BytesText::new(c)))
                    .map_err(|e| XmlError::Emit(e.to_string()))?;
            }
            Node::Pi { target, data } => {
                let content = if data.is_empty() {
                    target.clone()
                } else {
                    format!("{target} {data}")
                };
                w.write_event(Event::PI(BytesPI::new(content)))
                    .map_err(|e| XmlError::Emit(e.to_string()))?;
            }
        }
    }

    if opts.pretty && has_element_children {
        let pad = indent.repeat(depth);
        w.write_event(Event::Text(BytesText::new(&format!("\n{pad}"))))
            .map_err(|e| XmlError::Emit(e.to_string()))?;
    }

    w.write_event(Event::End(BytesEnd::new(qname.as_str())))
        .map_err(|e| XmlError::Emit(e.to_string()))?;

    if !el.tail.is_empty() {
        w.write_event(Event::Text(BytesText::new(&el.tail)))
            .map_err(|e| XmlError::Emit(e.to_string()))?;
    }
    Ok(())
}

/// Serialize a document to XML string.
pub fn to_string_doc(doc: &Document, opts: &XmlOpts) -> Result<String, XmlError> {
    let mut w = Writer::new(Cursor::new(Vec::new()));

    if opts.xml_declaration {
        let version = doc.version.as_deref().unwrap_or("1.0");
        let enc = opts.encoding.as_deref().or(doc.encoding.as_deref());
        let decl = if let Some(e) = enc {
            BytesDecl::new(version, Some(e), None)
        } else {
            BytesDecl::new(version, None, None)
        };
        w.write_event(Event::Decl(decl))
            .map_err(|e| XmlError::Emit(e.to_string()))?;
        if opts.pretty {
            w.write_event(Event::Text(BytesText::new("\n")))
                .map_err(|e| XmlError::Emit(e.to_string()))?;
        }
    }

    if let Some(root) = &doc.root {
        write_element(&mut w, root, opts, 0)?;
    }

    if opts.pretty {
        w.write_event(Event::Text(BytesText::new("\n")))
            .map_err(|e| XmlError::Emit(e.to_string()))?;
    }

    let bytes = w.into_inner().into_inner();
    String::from_utf8(bytes).map_err(|e| XmlError::Emit(e.to_string()))
}

/// Serialize an element subtree.
pub fn to_string_element(el: &Element, opts: &XmlOpts) -> Result<String, XmlError> {
    let mut w = Writer::new(Cursor::new(Vec::new()));
    write_element(&mut w, el, opts, 0)?;
    let bytes = w.into_inner().into_inner();
    String::from_utf8(bytes).map_err(|e| XmlError::Emit(e.to_string()))
}

/// Pretty-print with indentation (default two spaces).
pub fn pretty(el: &Element, indent: Option<&str>) -> Result<String, XmlError> {
    let mut opts = XmlOpts::default();
    opts.pretty = true;
    opts.xml_declaration = false;
    opts.indent = Some(indent.unwrap_or("  ").to_string());
    to_string_element(el, &opts)
}

pub fn pretty_doc(doc: &Document, indent: Option<&str>) -> Result<String, XmlError> {
    let mut opts = XmlOpts::default();
    opts.pretty = true;
    opts.indent = Some(indent.unwrap_or("  ").to_string());
    to_string_doc(doc, &opts)
}
