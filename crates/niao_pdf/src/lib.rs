//! PDF create (text, images, tables), extract text/pages, merge/split.
//! (~reportlab + pypdf subset)

mod build;
mod error;
mod extract;
mod lopdf_ops;
mod merge;
mod parallel;
mod read;
mod table;

pub use build::{
    add_page, close_builder, create_builder, finish_builder, image, line, rect, table, text,
    write_builder, BuilderStore, BuiltinFontChoice, CreateOpts, ImageOpts, LineOpts, RectOpts,
    TextOpts, DEFAULT_PAGE_HEIGHT, DEFAULT_PAGE_WIDTH,
};
pub use error::{PdfError, PdfResult};
pub use extract::{
    extract_page_text, extract_text_bytes, extract_text_doc, pages_text, ExtractOpts,
};
pub use merge::{
    copy_pages, extract_pages_bytes, merge_bytes, merge_docs, split_all, split_ranges,
};
pub use parallel::{parallel_extract_text, parallel_merge};
pub use read::{
    close_doc, is_valid, metadata, open_bytes, open_file, page_count, page_size, remove_pages,
    rotate_page, save_bytes, write_file, DocumentStore, PageSize, PdfMetadata,
};
pub use table::TableOpts;
