//! Parallel merge and text extraction.

use crate::error::PdfResult;
use crate::extract::{extract_text_bytes, ExtractOpts};
use crate::merge::merge_bytes;
use niao_parallel::try_map;

/// Parallel text extraction from many PDF byte blobs.
///
/// >>> parallel_extract_text(&[], &ExtractOpts::default(), 1).unwrap().is_empty()
/// true
pub fn parallel_extract_text(
    inputs: &[Vec<u8>],
    opts: &ExtractOpts,
    threads: usize,
) -> PdfResult<Vec<String>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let work = |bytes: &Vec<u8>| extract_text_bytes(bytes, opts);
    if threads <= 1 || inputs.len() == 1 {
        inputs.iter().map(work).collect()
    } else {
        try_map(inputs, threads, work)
    }
}

/// Parallel merge of PDF groups; each inner slice is merged into one output PDF.
pub fn parallel_merge(groups: &[Vec<Vec<u8>>], threads: usize) -> PdfResult<Vec<Vec<u8>>> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let work = |parts: &Vec<Vec<u8>>| merge_bytes(parts);
    if threads <= 1 || groups.len() == 1 {
        groups.iter().map(work).collect()
    } else {
        try_map(groups, threads, work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{create_builder, finish_builder, text, BuilderStore, CreateOpts, TextOpts};

    #[test]
    fn parallel_extract() {
        let mut builders = BuilderStore::new();
        let mut pdfs = Vec::new();
        for label in ["a", "b", "c"] {
            let b = create_builder(&mut builders, &CreateOpts::default()).unwrap();
            text(
                &mut builders,
                b,
                label,
                &TextOpts {
                    x: 72.0,
                    y: 700.0,
                    size: 12.0,
                    ..Default::default()
                },
            )
            .unwrap();
            pdfs.push(finish_builder(&mut builders, b).unwrap());
        }
        let texts = parallel_extract_text(&pdfs, &ExtractOpts::default(), 4).unwrap();
        assert_eq!(texts.len(), 3);
    }
}
