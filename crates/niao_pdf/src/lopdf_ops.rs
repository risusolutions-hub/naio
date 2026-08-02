//! Low-level lopdf merge and page extraction.

use crate::error::{PdfError, PdfResult};
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;

/// Merge multiple PDF documents into one.
pub fn merge_documents(mut docs: Vec<Document>) -> PdfResult<Document> {
    if docs.is_empty() {
        return Err(PdfError::InvalidInput(
            "merge requires at least one PDF".into(),
        ));
    }
    if docs.len() == 1 {
        return Ok(docs.remove(0));
    }

    let mut max_id = 1u32;
    let mut documents_pages: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut documents_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut document = Document::with_version("1.5");

    for mut doc in docs {
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        for (_, object_id) in doc.get_pages() {
            let object = doc
                .get_object(object_id)
                .map_err(|e| PdfError::Lopdf(e.to_string()))?
                .to_owned();
            documents_pages.insert(object_id, object);
        }
        documents_objects.extend(doc.objects);
    }

    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (object_id, object) in documents_objects {
        match object.type_name().unwrap_or("") {
            "Catalog" => {
                catalog_object = Some((
                    catalog_object.map(|(id, _)| id).unwrap_or(object_id),
                    object,
                ));
            }
            "Pages" => {
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref object)) = pages_object {
                        if let Ok(old_dictionary) = object.as_dict() {
                            dictionary.extend(old_dictionary);
                        }
                    }
                    pages_object = Some((
                        pages_object.map(|(id, _)| id).unwrap_or(object_id),
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            "Page" | "Outlines" | "Outline" => {}
            _ => {
                document.objects.insert(object_id, object);
            }
        }
    }

    let Some((page_id, page_object)) = pages_object else {
        return Err(PdfError::Lopdf("Pages root not found during merge".into()));
    };
    let Some((catalog_id, catalog_object)) = catalog_object else {
        return Err(PdfError::Lopdf(
            "Catalog root not found during merge".into(),
        ));
    };

    for (object_id, object) in documents_pages.iter() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", page_id);
            document
                .objects
                .insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    if let Ok(dictionary) = page_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Count", documents_pages.len() as u32);
        dictionary.set(
            "Kids",
            documents_pages
                .keys()
                .copied()
                .map(Object::Reference)
                .collect::<Vec<_>>(),
        );
        document
            .objects
            .insert(page_id, Object::Dictionary(dictionary));
    }

    if let Ok(dictionary) = catalog_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", page_id);
        dictionary.remove(b"Outlines");
        document
            .objects
            .insert(catalog_id, Object::Dictionary(dictionary));
    }

    document.trailer.set("Root", catalog_id);
    document.max_id = document.objects.len() as u32;
    document.renumber_objects();
    document.adjust_zero_pages();
    Ok(document)
}

/// Keep only the given lopdf page-number keys; remove all others.
pub fn extract_pages(doc: &Document, keep_page_numbers: &[u32]) -> PdfResult<Document> {
    if keep_page_numbers.is_empty() {
        return Err(PdfError::InvalidInput(
            "extract_pages requires at least one page".into(),
        ));
    }
    let mut filtered = doc.clone();
    let all_nums: Vec<u32> = filtered.get_pages().keys().copied().collect();
    for num in &all_nums {
        if !keep_page_numbers.contains(num) {
            if !filtered.get_pages().contains_key(num) {
                continue;
            }
        }
    }
    let remove: Vec<u32> = all_nums
        .into_iter()
        .filter(|n| !keep_page_numbers.contains(n))
        .collect();
    if !remove.is_empty() {
        filtered.delete_pages(&remove);
    }
    if filtered.get_pages().is_empty() {
        return Err(PdfError::InvalidInput(
            "no pages left after extraction".into(),
        ));
    }
    Ok(filtered)
}

pub fn document_to_bytes(doc: &mut Document) -> PdfResult<Vec<u8>> {
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Stream};

    fn one_page_doc(label: &str) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![72.into(), 720.into()]),
                Operation::new("Tj", vec![Object::string_literal(label)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    #[test]
    fn merge_two() {
        let merged = merge_documents(vec![one_page_doc("a"), one_page_doc("b")]).unwrap();
        assert_eq!(merged.get_pages().len(), 2);
    }
}
