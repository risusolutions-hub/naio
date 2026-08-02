//! Tree walking: children, ancestors, descendants, siblings.

use crate::error::{HtmlError, HtmlResult};
use crate::parse::{element_from_packed, pack_node, DocumentStore};
use scraper::node::Node;
use scraper::ElementRef;

pub fn parent(store: &DocumentStore, packed: i64) -> HtmlResult<Option<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .parent()
        .and_then(|p| pack_node(doc, doc_id, p.id()).ok()))
}

pub fn children(store: &DocumentStore, packed: i64) -> HtmlResult<Vec<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .children()
        .map(|c| pack_node(doc, doc_id, c.id()))
        .collect::<HtmlResult<Vec<_>>>()?)
}

pub fn child_elements(store: &DocumentStore, packed: i64) -> HtmlResult<Vec<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .child_elements()
        .map(|c| pack_node(doc, doc_id, c.id()))
        .collect::<HtmlResult<Vec<_>>>()?)
}

pub fn descendants(store: &DocumentStore, packed: i64) -> HtmlResult<Vec<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .descendants()
        .skip(1)
        .map(|d| pack_node(doc, doc_id, d.id()))
        .collect::<HtmlResult<Vec<_>>>()?)
}

pub fn ancestors(store: &DocumentStore, packed: i64) -> HtmlResult<Vec<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .ancestors()
        .skip(1)
        .map(|a| pack_node(doc, doc_id, a.id()))
        .collect::<HtmlResult<Vec<_>>>()?)
}

pub fn next_sibling(store: &DocumentStore, packed: i64) -> HtmlResult<Option<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .next_sibling()
        .and_then(|s| pack_node(doc, doc_id, s.id()).ok()))
}

pub fn prev_sibling(store: &DocumentStore, packed: i64) -> HtmlResult<Option<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .prev_sibling()
        .and_then(|s| pack_node(doc, doc_id, s.id()).ok()))
}

pub fn siblings(store: &DocumentStore, packed: i64) -> HtmlResult<Vec<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    let parent = match el.parent() {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };
    Ok(parent
        .children()
        .filter(|c| c.id() != el.id())
        .map(|s| pack_node(doc, doc_id, s.id()))
        .collect::<HtmlResult<Vec<_>>>()?)
}

/// Find first descendant element by tag name (optional attribute filter).
pub fn find(
    store: &DocumentStore,
    packed: i64,
    tag: &str,
    attr_key: Option<&str>,
    attr_val: Option<&str>,
) -> HtmlResult<Option<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    for d in el.descendent_elements() {
        if matches_element(d, tag, attr_key, attr_val) {
            return Ok(Some(pack_node(doc, doc_id, d.id())?));
        }
    }
    Ok(None)
}

/// Find all descendant elements by tag name (optional attribute filter).
pub fn find_all(
    store: &DocumentStore,
    packed: i64,
    tag: &str,
    attr_key: Option<&str>,
    attr_val: Option<&str>,
) -> HtmlResult<Vec<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(el
        .descendent_elements()
        .filter(|d| matches_element(*d, tag, attr_key, attr_val))
        .map(|d| pack_node(doc, doc_id, d.id()))
        .collect::<HtmlResult<Vec<_>>>()?)
}

fn matches_element(
    el: ElementRef<'_>,
    tag: &str,
    attr_key: Option<&str>,
    attr_val: Option<&str>,
) -> bool {
    let name = el.value().name();
    if !tag.is_empty() && !name.eq_ignore_ascii_case(tag) {
        return false;
    }
    match (attr_key, attr_val) {
        (None, _) => true,
        (Some(k), None) => el.value().attr(k).is_some(),
        (Some(k), Some(v)) => el.value().attr(k).map(|a| a == v).unwrap_or(false),
    }
}

pub fn node_type(store: &DocumentStore, packed: i64) -> HtmlResult<&'static str> {
    let (doc_id, index) = crate::parse::unpack_node(packed)?;
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

    #[test]
    fn walk_descendants() {
        let mut store = DocumentStore::new();
        let id = alloc_document(&mut store, "<div><p><span>x</span></p></div>", false);
        let root = root_node(&store, id).unwrap();
        let desc = descendants(&store, root).unwrap();
        assert!(desc.len() >= 2);
    }
}
