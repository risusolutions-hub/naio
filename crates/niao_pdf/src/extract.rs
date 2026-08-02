//! Text extraction from PDF documents.

use crate::error::{PdfError, PdfResult};
use crate::read::{page_count, DocumentStore};
use pdf_extract::extract_text_from_mem;

/// Options for bulk text extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractOpts {
    /// When set, only extract these 0-based page indices.
    pub pages: Option<Vec<usize>>,
    /// Insert separator between pages (default `"\n\n"`).
    pub page_separator: String,
}

/// Extract all text from PDF bytes (one-shot, no handle).
///
/// >>> extract_text_bytes(b"", &ExtractOpts::default()).is_err()
/// true
pub fn extract_text_bytes(bytes: &[u8], opts: &ExtractOpts) -> PdfResult<String> {
    if bytes.is_empty() {
        return Err(PdfError::InvalidInput("empty PDF bytes".into()));
    }
    if let Some(pages) = &opts.pages {
        let mut store = DocumentStore::new();
        let id = crate::read::open_bytes(&mut store, bytes)?;
        let out = extract_text_doc(&store, id, opts)?;
        let _ = crate::read::close_doc(&mut store, id);
        let _ = pages;
        Ok(out)
    } else {
        extract_text_from_mem(bytes).map_err(|e| PdfError::Extract(e.to_string()))
    }
}

/// Extract text from an open document.
pub fn extract_text_doc(store: &DocumentStore, id: i64, opts: &ExtractOpts) -> PdfResult<String> {
    let bytes = crate::read::save_bytes(store, id)?;
    if let Some(pages) = &opts.pages {
        let count = page_count(store, id)?;
        let mut parts = Vec::with_capacity(pages.len());
        for &p in pages {
            if p >= count {
                return Err(PdfError::InvalidPage(p));
            }
            let page_bytes = crate::merge::extract_pages_bytes(store, id, &[p])?;
            let text =
                extract_text_from_mem(&page_bytes).map_err(|e| PdfError::Extract(e.to_string()))?;
            parts.push(text);
        }
        Ok(parts.join(&opts.page_separator))
    } else {
        extract_text_from_mem(&bytes).map_err(|e| PdfError::Extract(e.to_string()))
    }
}

/// Extract text for a single 0-based page index.
pub fn extract_page_text(store: &DocumentStore, id: i64, page: usize) -> PdfResult<String> {
    let opts = ExtractOpts {
        pages: Some(vec![page]),
        page_separator: String::new(),
    };
    extract_text_doc(store, id, &opts)
}

/// Per-page text as a vector (index aligns with page numbers).
pub fn pages_text(store: &DocumentStore, id: i64) -> PdfResult<Vec<String>> {
    let count = page_count(store, id)?;
    let mut out = Vec::with_capacity(count);
    for page in 0..count {
        out.push(extract_page_text(store, id, page)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{create_builder, finish_builder, text, BuilderStore, CreateOpts, TextOpts};

    fn sample_pdf() -> Vec<u8> {
        let mut builders = BuilderStore::new();
        let b = create_builder(&mut builders, &CreateOpts::default()).unwrap();
        text(
            &mut builders,
            b,
            "alpha beta",
            &TextOpts {
                x: 72.0,
                y: 700.0,
                size: 14.0,
                ..Default::default()
            },
        )
        .unwrap();
        finish_builder(&mut builders, b).unwrap()
    }

    #[test]
    fn extract_contains_words() {
        let bytes = sample_pdf();
        let text = extract_text_bytes(&bytes, &ExtractOpts::default()).unwrap();
        assert!(text.contains("alpha") || text.contains("beta"));
    }
}
