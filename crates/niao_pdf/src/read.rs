//! Open PDF documents, metadata, page info, and serialization.

use crate::error::{PdfError, PdfResult};
use crate::lopdf_ops::document_to_bytes;
use lopdf::{Document, Object, ObjectId};
use std::collections::HashMap;
use std::path::Path;

/// Metadata fields extracted from a PDF Info dictionary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
}

/// Page dimensions in points (1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f64,
    pub height: f64,
}

/// In-memory PDF document store keyed by positive handles.
pub struct DocumentStore {
    next_id: i64,
    docs: HashMap<i64, Document>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            docs: HashMap::new(),
        }
    }

    pub fn insert(&mut self, doc: Document) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.docs.insert(id, doc);
        id
    }

    pub fn get(&self, id: i64) -> Option<&Document> {
        self.docs.get(&id)
    }

    pub fn get_mut(&mut self, id: i64) -> Option<&mut Document> {
        self.docs.get_mut(&id)
    }

    pub fn remove(&mut self, id: i64) -> bool {
        self.docs.remove(&id).is_some()
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Return true when bytes parse as a PDF.
///
/// >>> is_valid(b"not a pdf")
/// false
pub fn is_valid(bytes: &[u8]) -> bool {
    Document::load_mem(bytes).is_ok()
}

/// Open a PDF from bytes.
pub fn open_bytes(store: &mut DocumentStore, bytes: &[u8]) -> PdfResult<i64> {
    let doc = Document::load_mem(bytes)?;
    Ok(store.insert(doc))
}

/// Open a PDF from a filesystem path.
pub fn open_file(store: &mut DocumentStore, path: &Path) -> PdfResult<i64> {
    let doc = Document::load(path)?;
    Ok(store.insert(doc))
}

/// Release a document handle.
pub fn close_doc(store: &mut DocumentStore, id: i64) -> PdfResult<()> {
    if store.remove(id) {
        Ok(())
    } else {
        Err(PdfError::InvalidHandle)
    }
}

/// Number of pages (0 when empty).
pub fn page_count(store: &DocumentStore, id: i64) -> PdfResult<usize> {
    let doc = store.get(id).ok_or(PdfError::InvalidHandle)?;
    Ok(doc.get_pages().len())
}

fn page_list(doc: &Document) -> Vec<(u32, ObjectId)> {
    let mut pages: Vec<_> = doc.get_pages().into_iter().collect();
    pages.sort_by_key(|(num, _)| *num);
    pages
}

pub(crate) fn resolve_page_index(doc: &Document, page: usize) -> PdfResult<u32> {
    let pages = page_list(doc);
    pages
        .get(page)
        .map(|(num, _)| *num)
        .ok_or(PdfError::InvalidPage(page))
}

fn resolve_page_object(doc: &Document, page: usize) -> PdfResult<ObjectId> {
    let pages = page_list(doc);
    pages
        .get(page)
        .map(|(_, id)| *id)
        .ok_or(PdfError::InvalidPage(page))
}

fn object_to_string(obj: &Object) -> Option<String> {
    match obj {
        Object::String(bytes, _) => String::from_utf8(bytes.clone()).ok(),
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

fn media_box_from_page(doc: &Document, page_id: ObjectId) -> PdfResult<PageSize> {
    let dict = doc
        .get_dictionary(page_id)
        .map_err(|e| PdfError::Lopdf(e.to_string()))?;
    let media = dict
        .get(b"MediaBox")
        .map_err(|e| PdfError::Lopdf(e.to_string()))?;
    let Object::Array(vals) = media else {
        return Err(PdfError::Lopdf("MediaBox is not an array".into()));
    };
    if vals.len() < 4 {
        return Err(PdfError::Lopdf("MediaBox array too short".into()));
    }
    let w = object_as_f64(&vals[2])? - object_as_f64(&vals[0])?;
    let h = object_as_f64(&vals[3])? - object_as_f64(&vals[1])?;
    Ok(PageSize {
        width: w,
        height: h,
    })
}

fn object_as_f64(obj: &Object) -> PdfResult<f64> {
    match obj {
        Object::Integer(n) => Ok(*n as f64),
        Object::Real(n) => Ok(*n as f64),
        other => Err(PdfError::Lopdf(format!(
            "expected numeric MediaBox component, got {other:?}"
        ))),
    }
}

/// Page size in points for `page` (0-based index).
pub fn page_size(store: &DocumentStore, id: i64, page: usize) -> PdfResult<PageSize> {
    let doc = store.get(id).ok_or(PdfError::InvalidHandle)?;
    let page_id = resolve_page_object(doc, page)?;
    media_box_from_page(doc, page_id)
}

/// Document Info dictionary as structured metadata.
pub fn metadata(store: &DocumentStore, id: i64) -> PdfResult<PdfMetadata> {
    let doc = store.get(id).ok_or(PdfError::InvalidHandle)?;
    let mut meta = PdfMetadata::default();
    let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) else {
        return Ok(meta);
    };
    let Ok(dict) = doc.get_dictionary(info_ref) else {
        return Ok(meta);
    };
    meta.title = dict.get(b"Title").ok().and_then(|o| object_to_string(o));
    meta.author = dict.get(b"Author").ok().and_then(|o| object_to_string(o));
    meta.subject = dict.get(b"Subject").ok().and_then(|o| object_to_string(o));
    meta.keywords = dict.get(b"Keywords").ok().and_then(|o| object_to_string(o));
    meta.creator = dict.get(b"Creator").ok().and_then(|o| object_to_string(o));
    meta.producer = dict.get(b"Producer").ok().and_then(|o| object_to_string(o));
    meta.creation_date = dict
        .get(b"CreationDate")
        .ok()
        .and_then(|o| object_to_string(o));
    meta.modification_date = dict.get(b"ModDate").ok().and_then(|o| object_to_string(o));
    Ok(meta)
}

/// Serialize document to bytes.
pub fn save_bytes(store: &DocumentStore, id: i64) -> PdfResult<Vec<u8>> {
    let mut doc = store.get(id).ok_or(PdfError::InvalidHandle)?.clone();
    document_to_bytes(&mut doc)
}

/// Write document to a filesystem path.
pub fn write_file(store: &DocumentStore, id: i64, path: &Path) -> PdfResult<()> {
    let bytes = save_bytes(store, id)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Rotate a page by 90/180/270 degrees clockwise.
pub fn rotate_page(store: &mut DocumentStore, id: i64, page: usize, degrees: i32) -> PdfResult<()> {
    let doc = store.get_mut(id).ok_or(PdfError::InvalidHandle)?;
    let page_id = resolve_page_object(doc, page)?;
    let rot = match degrees.rem_euclid(360) {
        0 => 0,
        90 => 90,
        180 => 180,
        270 => 270,
        other => {
            return Err(PdfError::InvalidInput(format!(
                "rotate degrees must be a multiple of 90, got {other}"
            )));
        }
    };
    let dict = doc
        .get_dictionary_mut(page_id)
        .map_err(|e| PdfError::Lopdf(e.to_string()))?;
    if rot == 0 {
        dict.remove(b"Rotate");
    } else {
        dict.set("Rotate", rot as i32);
    }
    Ok(())
}

/// Remove pages by 0-based indices.
pub fn remove_pages(store: &mut DocumentStore, id: i64, pages: &[usize]) -> PdfResult<()> {
    let doc = store.get_mut(id).ok_or(PdfError::InvalidHandle)?;
    let mut nums: Vec<u32> = Vec::new();
    for &p in pages {
        nums.push(resolve_page_index(doc, p)?);
    }
    nums.sort_unstable();
    nums.dedup();
    if !nums.is_empty() {
        doc.delete_pages(&nums);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{create_builder, finish_builder, text, BuilderStore, CreateOpts, TextOpts};

    #[test]
    fn roundtrip_open_save() {
        let mut builders = BuilderStore::new();
        let b = create_builder(&mut builders, &CreateOpts::default()).unwrap();
        text(
            &mut builders,
            b,
            "hello",
            &TextOpts {
                x: 72.0,
                y: 720.0,
                size: 12.0,
                ..Default::default()
            },
        )
        .unwrap();
        let bytes = finish_builder(&mut builders, b).unwrap();
        let mut store = DocumentStore::new();
        let id = open_bytes(&mut store, &bytes).unwrap();
        assert_eq!(page_count(&store, id).unwrap(), 1);
        close_doc(&mut store, id).unwrap();
    }
}
