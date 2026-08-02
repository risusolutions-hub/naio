//! CSS selector compilation and matching.

use crate::error::{HtmlError, HtmlResult};
use crate::parse::{element_from_packed, pack_node, DocumentStore};
use scraper::Selector;
use std::collections::HashMap;

/// Compiled CSS selector store.
#[derive(Default)]
pub struct SelectorStore {
    next_id: i64,
    selectors: HashMap<i64, Selector>,
}

impl SelectorStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            selectors: HashMap::new(),
        }
    }

    pub fn get(&self, id: i64) -> Option<&Selector> {
        self.selectors.get(&id)
    }

    pub fn insert(&mut self, sel: Selector) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.selectors.insert(id, sel);
        id
    }

    pub fn remove(&mut self, id: i64) -> bool {
        self.selectors.remove(&id).is_some()
    }
}

/// Parse a CSS selector string.
pub fn parse_selector(css: &str) -> HtmlResult<Selector> {
    Selector::parse(css).map_err(|e| HtmlError::Selector(format!("{e}")))
}

/// Return true when `css` is valid selector syntax.
pub fn valid_selector(css: &str) -> bool {
    Selector::parse(css).is_ok()
}

/// Compile a selector → handle for repeated queries.
pub fn compile_selector(store: &mut SelectorStore, css: &str) -> HtmlResult<i64> {
    let sel = parse_selector(css)?;
    Ok(store.insert(sel))
}

/// Select all matching elements under `node` (includes the subtree rooted at `node`).
pub fn select_nodes(store: &DocumentStore, packed: i64, css: &str) -> HtmlResult<Vec<i64>> {
    let sel = parse_selector(css)?;
    select_with_selector(store, packed, &sel)
}

pub fn select_with_handle(
    store: &DocumentStore,
    sel_store: &SelectorStore,
    packed: i64,
    sel_id: i64,
) -> HtmlResult<Vec<i64>> {
    let sel = sel_store
        .get(sel_id)
        .ok_or_else(|| HtmlError::InvalidHandle(format!("invalid selector handle {sel_id}")))?;
    select_with_selector(store, packed, sel)
}

fn select_with_selector(
    store: &DocumentStore,
    packed: i64,
    sel: &Selector,
) -> HtmlResult<Vec<i64>> {
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    let mut out = Vec::new();
    for m in el.select(sel) {
        out.push(pack_node(doc, doc_id, m.id())?);
    }
    if out.is_empty() && el.id() == doc.html.tree.root().id() {
        for m in doc.html.select(sel) {
            out.push(pack_node(doc, doc_id, m.id())?);
        }
    }
    Ok(out)
}

pub fn select_one(store: &DocumentStore, packed: i64, css: &str) -> HtmlResult<Option<i64>> {
    Ok(select_nodes(store, packed, css)?.into_iter().next())
}

/// True when the element matches `css` in document context.
pub fn matches(store: &DocumentStore, packed: i64, css: &str) -> HtmlResult<bool> {
    let sel = parse_selector(css)?;
    let (doc_id, el) = element_from_packed(store, packed)?;
    let doc = store.get(doc_id).unwrap();
    Ok(doc.html.select(&sel).any(|m| m.id() == el.id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{alloc_document, root_node};

    #[test]
    fn select_links() {
        let mut store = DocumentStore::new();
        let id = alloc_document(
            &mut store,
            r#"<html><body><a href="/">home</a><a href="/x">x</a></body></html>"#,
            false,
        );
        let root = root_node(&store, id).unwrap();
        let nodes = select_nodes(&store, root, "a").unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn valid_selector_check() {
        assert!(valid_selector("div.foo"));
        assert!(!valid_selector("[[["));
    }
}
