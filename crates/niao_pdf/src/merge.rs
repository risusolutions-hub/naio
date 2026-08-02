//! Merge, split, and page extraction.

use crate::error::{PdfError, PdfResult};
use crate::lopdf_ops::{document_to_bytes, extract_pages as lopdf_extract, merge_documents};
use crate::read::{page_count, resolve_page_index, DocumentStore};
use lopdf::Document;

/// Merge multiple PDF byte blobs into one document.
///
/// >>> merge_bytes(&[]).is_err()
/// true
pub fn merge_bytes(inputs: &[Vec<u8>]) -> PdfResult<Vec<u8>> {
    if inputs.is_empty() {
        return Err(PdfError::InvalidInput(
            "merge requires at least one PDF".into(),
        ));
    }
    let mut docs = Vec::with_capacity(inputs.len());
    for chunk in inputs {
        docs.push(Document::load_mem(chunk)?);
    }
    let mut merged = merge_documents(docs)?;
    document_to_bytes(&mut merged)
}

/// Merge open document handles.
pub fn merge_docs(store: &DocumentStore, ids: &[i64]) -> PdfResult<Vec<u8>> {
    if ids.is_empty() {
        return Err(PdfError::InvalidInput(
            "merge requires at least one document".into(),
        ));
    }
    let mut docs = Vec::with_capacity(ids.len());
    for &id in ids {
        let doc = store.get(id).ok_or(PdfError::InvalidHandle)?;
        docs.push(doc.clone());
    }
    let mut merged = merge_documents(docs)?;
    document_to_bytes(&mut merged)
}

/// Extract selected pages into a new PDF byte blob.
pub fn extract_pages_bytes(store: &DocumentStore, id: i64, pages: &[usize]) -> PdfResult<Vec<u8>> {
    if pages.is_empty() {
        return Err(PdfError::InvalidInput(
            "extract_pages requires at least one page".into(),
        ));
    }
    let doc = store.get(id).ok_or(PdfError::InvalidHandle)?;
    let count = page_count(store, id)?;
    let mut page_nums = Vec::with_capacity(pages.len());
    for &p in pages {
        if p >= count {
            return Err(PdfError::InvalidPage(p));
        }
        page_nums.push(resolve_page_index(doc, p)?);
    }
    page_nums.sort_unstable();
    page_nums.dedup();
    let mut out = lopdf_extract(doc, &page_nums)?;
    document_to_bytes(&mut out)
}

/// Split a document into multiple PDFs by page ranges (inclusive, 0-based).
pub fn split_ranges(
    store: &DocumentStore,
    id: i64,
    ranges: &[(usize, usize)],
) -> PdfResult<Vec<Vec<u8>>> {
    if ranges.is_empty() {
        return Err(PdfError::InvalidInput(
            "split requires at least one range".into(),
        ));
    }
    let count = page_count(store, id)?;
    let mut outputs = Vec::with_capacity(ranges.len());
    for &(start, end) in ranges {
        if start > end || end >= count {
            return Err(PdfError::InvalidInput(format!(
                "invalid page range [{start}, {end}] for document with {count} page(s)"
            )));
        }
        let pages: Vec<usize> = (start..=end).collect();
        outputs.push(extract_pages_bytes(store, id, &pages)?);
    }
    Ok(outputs)
}

/// Split every page into its own single-page PDF.
pub fn split_all(store: &DocumentStore, id: i64) -> PdfResult<Vec<Vec<u8>>> {
    let count = page_count(store, id)?;
    let ranges: Vec<(usize, usize)> = (0..count).map(|p| (p, p)).collect();
    split_ranges(store, id, &ranges)
}

/// Copy pages from `src` into a new document handle in `store`.
pub fn copy_pages(store: &mut DocumentStore, src: i64, pages: &[usize]) -> PdfResult<i64> {
    let bytes = extract_pages_bytes(store, src, pages)?;
    crate::read::open_bytes(store, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{
        add_page, create_builder, finish_builder, text, BuilderStore, CreateOpts, TextOpts,
    };
    use crate::read::open_bytes;

    fn two_page_pdf() -> Vec<u8> {
        let mut builders = BuilderStore::new();
        let b = create_builder(&mut builders, &CreateOpts::default()).unwrap();
        text(
            &mut builders,
            b,
            "p1",
            &TextOpts {
                x: 72.0,
                y: 700.0,
                size: 12.0,
                ..Default::default()
            },
        )
        .unwrap();
        add_page(&mut builders, b, None).unwrap();
        text(
            &mut builders,
            b,
            "p2",
            &TextOpts {
                x: 72.0,
                y: 700.0,
                size: 12.0,
                ..Default::default()
            },
        )
        .unwrap();
        finish_builder(&mut builders, b).unwrap()
    }

    #[test]
    fn split_and_merge() {
        let bytes = two_page_pdf();
        let mut store = DocumentStore::new();
        let id = open_bytes(&mut store, &bytes).unwrap();
        let parts = split_all(&store, id).unwrap();
        assert_eq!(parts.len(), 2);
        let merged = merge_bytes(&parts).unwrap();
        let id2 = open_bytes(&mut store, &merged).unwrap();
        assert_eq!(page_count(&store, id2).unwrap(), 2);
    }
}
