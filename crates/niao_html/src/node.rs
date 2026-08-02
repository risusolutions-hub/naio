//! Element metadata: tag, attributes, classes.

use crate::error::{HtmlError, HtmlResult};
use crate::parse::{element_from_packed, unpack_node, DocumentStore};
use scraper::node::Node;

pub fn tag(store: &DocumentStore, packed: i64) -> HtmlResult<String> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el.value().name().to_string())
}

pub fn attr(store: &DocumentStore, packed: i64, name: &str) -> HtmlResult<Option<String>> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el.value().attr(name).map(str::to_string))
}

pub fn has_attr(store: &DocumentStore, packed: i64, name: &str) -> HtmlResult<bool> {
    Ok(attr(store, packed, name)?.is_some())
}

pub fn attrs(store: &DocumentStore, packed: i64) -> HtmlResult<Vec<(String, String)>> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el
        .value()
        .attrs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect())
}

pub fn id_attr(store: &DocumentStore, packed: i64) -> HtmlResult<Option<String>> {
    attr(store, packed, "id")
}

pub fn classes(store: &DocumentStore, packed: i64) -> HtmlResult<Vec<String>> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el.value().classes().map(str::to_string).collect())
}

pub fn has_class(store: &DocumentStore, packed: i64, class_name: &str) -> HtmlResult<bool> {
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el.value().classes().any(|c| c == class_name))
}

pub fn is_element(store: &DocumentStore, packed: i64) -> HtmlResult<bool> {
    Ok(node_type_raw(store, packed)? == "element")
}

pub fn is_text(store: &DocumentStore, packed: i64) -> HtmlResult<bool> {
    Ok(node_type_raw(store, packed)? == "text")
}

pub fn is_comment(store: &DocumentStore, packed: i64) -> HtmlResult<bool> {
    Ok(node_type_raw(store, packed)? == "comment")
}

pub fn is_tag(store: &DocumentStore, packed: i64, name: &str) -> HtmlResult<bool> {
    if !is_element(store, packed)? {
        return Ok(false);
    }
    let (_, el) = element_from_packed(store, packed)?;
    Ok(el.value().name().eq_ignore_ascii_case(name))
}

fn node_type_raw(store: &DocumentStore, packed: i64) -> HtmlResult<&'static str> {
    let (doc_id, index) = unpack_node(packed)?;
    let doc = store
        .get(doc_id)
        .ok_or_else(|| HtmlError::InvalidHandle(format!("invalid document handle {doc_id}")))?;
    let node = doc.node_at(index)?;
    Ok(match node.value() {
        Node::Document => "document",
        Node::Fragment => "fragment",
        Node::Element(_) => "element",
        Node::Text(_) => "text",
        Node::Comment(_) => "comment",
        Node::Doctype(_) => "doctype",
        Node::ProcessingInstruction(_) => "processing_instruction",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{alloc_document, root_node};
    use crate::select::select_one;

    #[test]
    fn attrs_and_classes() {
        let mut store = DocumentStore::new();
        let id = alloc_document(&mut store, r#"<div id="main" class="a b">x</div>"#, false);
        let root = root_node(&store, id).unwrap();
        let div = select_one(&store, root, "div").unwrap().unwrap();
        assert_eq!(id_attr(&store, div).unwrap().as_deref(), Some("main"));
        let cls = classes(&store, div).unwrap();
        assert!(cls.contains(&"a".to_string()));
        assert!(has_class(&store, div, "b").unwrap());
    }
}
