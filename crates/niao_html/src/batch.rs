//! Parallel batch parse and select.

use crate::error::HtmlResult;
use crate::parse::{alloc_document, root_node, DocumentStore};
use crate::select::select_nodes;
use crate::text::{extract_text, TextOpts};
use niao_parallel::map as parallel_map;

/// Parse many HTML strings; returns document handles (caller must close).
pub fn parallel_parse(store: &mut DocumentStore, htmls: &[String], fragment: bool) -> Vec<i64> {
    htmls
        .iter()
        .map(|s| alloc_document(store, s, fragment))
        .collect()
}

/// For each HTML string, extract text matching optional selector (parallel).
pub fn parallel_extract_text(
    htmls: &[String],
    selector: Option<&str>,
    opts: &TextOpts,
    threads: usize,
) -> HtmlResult<Vec<String>> {
    let sel_css = selector.map(str::to_string);
    let results: Vec<HtmlResult<String>> = parallel_map(htmls, threads, |html| {
        extract_text(html, sel_css.as_deref(), opts)
    });
    results.into_iter().collect()
}

/// Parse each HTML and run CSS select; returns `(doc_id, node handles)` pairs.
pub fn parallel_select(
    store: &mut DocumentStore,
    htmls: &[String],
    css: &str,
    _threads: usize,
) -> HtmlResult<Vec<(i64, Vec<i64>)>> {
    let ids = parallel_parse(store, htmls, false);
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let root = root_node(store, id)?;
        let nodes = select_nodes(store, root, css)?;
        out.push((id, nodes));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_extract() {
        let htmls = vec!["<p>one</p>".to_string(), "<p>two</p>".to_string()];
        let opts = TextOpts::default();
        let texts = parallel_extract_text(&htmls, Some("p"), &opts, 2).unwrap();
        assert_eq!(texts.len(), 2);
    }
}
