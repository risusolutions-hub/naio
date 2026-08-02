//! HTML parsing and document handles.

use crate::error::{HtmlError, HtmlResult};
use ego_tree::NodeId;
use scraper::{Html, Node};
use std::collections::HashMap;

/// Stored document with a stable node index table.
pub struct StoredDoc {
    pub html: Html,
    /// Maps packed node index → tree node id.
    id_map: Vec<NodeId>,
}

impl StoredDoc {
    fn build(html: Html) -> Self {
        let id_map: Vec<NodeId> = html.tree.root().descendants().map(|n| n.id()).collect();
        Self { html, id_map }
    }

    pub fn node_at(&self, index: usize) -> HtmlResult<ego_tree::NodeRef<'_, Node>> {
        let id =
            self.id_map.get(index).copied().ok_or_else(|| {
                HtmlError::InvalidNode(format!("node index {index} out of range"))
            })?;
        self.html
            .tree
            .get(id)
            .ok_or_else(|| HtmlError::InvalidNode("stale node".into()))
    }

    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        self.id_map.iter().position(|x| *x == id)
    }

    pub fn root_element(&self) -> scraper::ElementRef<'_> {
        self.html.root_element()
    }
}

/// Parsed HTML document store (thread-local in runtime bindings).
pub struct DocumentStore {
    next_id: i64,
    docs: HashMap<i64, StoredDoc>,
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self {
            next_id: 1,
            docs: HashMap::new(),
        }
    }
}

impl DocumentStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: i64) -> Option<&StoredDoc> {
        self.docs.get(&id)
    }

    pub fn insert(&mut self, html: Html) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.docs.insert(id, StoredDoc::build(html));
        id
    }

    pub fn remove(&mut self, id: i64) -> bool {
        self.docs.remove(&id).is_some()
    }
}

/// Parse a full HTML document (forgiving HTML5 parser).
pub fn parse_document(html: &str) -> Html {
    Html::parse_document(html)
}

/// Parse an HTML fragment.
pub fn parse_fragment(html: &str) -> Html {
    Html::parse_fragment(html)
}

/// Allocate a document handle in `store`.
pub fn alloc_document(store: &mut DocumentStore, html: &str, fragment: bool) -> i64 {
    let doc = if fragment {
        parse_fragment(html)
    } else {
        parse_document(html)
    };
    store.insert(doc)
}

/// Pack `(doc_id, node_index)` into a single node handle for the Niao API.
pub fn pack_node(doc: &StoredDoc, doc_id: i64, node_id: NodeId) -> HtmlResult<i64> {
    let index = doc
        .index_of(node_id)
        .ok_or_else(|| HtmlError::InvalidNode("node not indexed".into()))?;
    Ok((doc_id << 32) | (index as i64))
}

/// Unpack a node handle.
pub fn unpack_node(packed: i64) -> HtmlResult<(i64, usize)> {
    if packed <= 0 {
        return Err(HtmlError::InvalidNode(format!(
            "invalid node handle {packed}"
        )));
    }
    let doc_id = packed >> 32;
    let index = (packed & 0xFFFF_FFFF) as usize;
    if doc_id <= 0 {
        return Err(HtmlError::InvalidNode(format!(
            "invalid node handle {packed}"
        )));
    }
    Ok((doc_id, index))
}

/// Resolve an element node from a packed handle.
pub fn element_from_packed<'a>(
    store: &'a DocumentStore,
    packed: i64,
) -> HtmlResult<(i64, scraper::ElementRef<'a>)> {
    let (doc_id, index) = unpack_node(packed)?;
    let doc = store
        .get(doc_id)
        .ok_or_else(|| HtmlError::InvalidHandle(format!("invalid document handle {doc_id}")))?;
    let nr = doc.node_at(index)?;
    let el = scraper::ElementRef::wrap(nr)
        .ok_or_else(|| HtmlError::InvalidNode("node is not an element".into()))?;
    Ok((doc_id, el))
}

/// Root element handle for a document.
pub fn root_node(store: &DocumentStore, doc_id: i64) -> HtmlResult<i64> {
    let doc = store
        .get(doc_id)
        .ok_or_else(|| HtmlError::InvalidHandle(format!("invalid document handle {doc_id}")))?;
    pack_node(doc, doc_id, doc.root_element().id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let mut store = DocumentStore::new();
        let id = alloc_document(&mut store, "<p>x</p>", false);
        let root = root_node(&store, id).unwrap();
        let (doc, idx) = unpack_node(root).unwrap();
        assert_eq!(doc, id);
        assert!(store.get(doc).unwrap().node_at(idx).is_ok());
    }
}
