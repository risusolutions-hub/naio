//! Native npdf standard library — PDF create (text, images, tables), extract
//! text/pages, merge/split (~reportlab + pypdf subset).
//!
//! Import with `import "npdf"` (or `import "std/npdf"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_parallel::available_threads;
use niao_pdf::{
    add_page, close_builder, close_doc, copy_pages, create_builder, extract_page_text,
    extract_pages_bytes, extract_text_bytes, extract_text_doc, finish_builder, image, is_valid,
    line, merge_bytes, merge_docs, metadata, open_bytes, open_file, page_count, page_size,
    pages_text, parallel_extract_text, parallel_merge, rect, remove_pages, rotate_page, save_bytes,
    split_all, split_ranges, table, text, write_builder, write_file, BuiltinFontChoice,
    BuilderStore, CreateOpts, DocumentStore, ExtractOpts, ImageOpts, LineOpts, PageSize,
    PdfError, PdfMetadata, RectOpts, TableOpts, TextOpts, DEFAULT_PAGE_HEIGHT, DEFAULT_PAGE_WIDTH,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

const E3550: u32 = codes::E3559_NPDF_ARITY;
const E3551: u32 = codes::E3560_NPDF_ERROR;
const E3552: u32 = codes::E3561_NPDF_TYPE;
const E3553: u32 = codes::E3562_NPDF_INVALID_HANDLE;

thread_local! {
    static DOCS: RefCell<DocumentStore> = RefCell::new(DocumentStore::new());
    static BUILDERS: RefCell<BuilderStore> = RefCell::new(BuilderStore::new());
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3552, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3550,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn npdf_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3551, "npdf_error", msg.into(), span)
}

fn invalid_handle(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3553, "npdf_error", msg.into(), span)
}

fn map_err(span: Span, err: PdfError) -> ValueRef {
    let code = match err {
        PdfError::InvalidHandle => E3553,
        _ => E3551,
    };
    error_value(code, "npdf_error", err.message(), span)
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn doc_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    let id = int_arg(args, idx, name, span)?;
    if id <= 0 {
        return Err(type_err(span, format!("{name}() expects a positive document handle")));
    }
    Ok(id)
}

fn builder_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    let id = int_arg(args, idx, name, span)?;
    if id <= 0 {
        return Err(type_err(span, format!("{name}() expects a positive builder handle")));
    }
    Ok(id)
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object_arg(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn float_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: f32) -> f32 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n as f32,
        Some(Value::Float(f)) => f as f32,
        _ => default,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: &str) -> String {
    let Some(map) = map else {
        return default.into();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => s,
        _ => default.into(),
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        _ => default,
    }
}

fn int_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<usize>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) if *n >= 0 => out.push(*n as usize),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects non-negative int array; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<Vec<u8>>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::ByteArray(b) => out.push(b.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects byte[][]; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a byte array list as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn table_rows_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<Vec<String>>> {
    match &*args[idx].borrow() {
        Value::Array(rows) => {
            let mut out = Vec::with_capacity(rows.len());
            for (ri, row) in rows.iter().enumerate() {
                match &*row.borrow() {
                    Value::Array(cells) => {
                        let mut row_out = Vec::with_capacity(cells.len());
                        for (ci, cell) in cells.iter().enumerate() {
                            match &*cell.borrow() {
                                Value::String(s) => row_out.push(s.clone()),
                                other => {
                                    return Err(type_err(
                                        span,
                                        format!(
                                            "{name}() row {} cell {} must be string, got {}",
                                            ri + 1,
                                            ci + 1,
                                            other.type_name()
                                        ),
                                    ));
                                }
                            }
                        }
                        out.push(row_out);
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() row {} must be string array, got {}",
                                ri + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a 2D string array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn rgb_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: (f32, f32, f32)) -> (f32, f32, f32) {
    let Some(map) = map else {
        return default;
    };
    let Some(v) = map.get(key) else {
        return default;
    };
    match &*v.borrow() {
        Value::Array(items) if items.len() >= 3 => {
            let r = match &*items[0].borrow() {
                Value::Int(n) => *n as f32,
                Value::Float(f) => *f as f32,
                _ => default.0,
            };
            let g = match &*items[1].borrow() {
                Value::Int(n) => *n as f32,
                Value::Float(f) => *f as f32,
                _ => default.1,
            };
            let b = match &*items[2].borrow() {
                Value::Int(n) => *n as f32,
                Value::Float(f) => *f as f32,
                _ => default.2,
            };
            (r, g, b)
        }
        _ => default,
    }
}

fn font_choice(name: &str) -> BuiltinFontChoice {
    match name.to_ascii_lowercase().as_str() {
        "helvetica-bold" | "helvetica_bold" => BuiltinFontChoice::HelveticaBold,
        "helvetica-oblique" | "helvetica_oblique" => BuiltinFontChoice::HelveticaOblique,
        "helvetica-boldoblique" | "helvetica_bold_oblique" => BuiltinFontChoice::HelveticaBoldOblique,
        "times" | "times-roman" => BuiltinFontChoice::Times,
        "times-bold" => BuiltinFontChoice::TimesBold,
        "times-italic" => BuiltinFontChoice::TimesItalic,
        "times-bolditalic" => BuiltinFontChoice::TimesBoldItalic,
        "courier" => BuiltinFontChoice::Courier,
        "courier-bold" => BuiltinFontChoice::CourierBold,
        "courier-oblique" => BuiltinFontChoice::CourierOblique,
        "courier-boldoblique" => BuiltinFontChoice::CourierBoldOblique,
        _ => BuiltinFontChoice::Helvetica,
    }
}

fn create_opts_from(map: Option<&HashMap<String, ValueRef>>) -> CreateOpts {
    CreateOpts {
        page_width: float_field(map, "page_width", DEFAULT_PAGE_WIDTH),
        page_height: float_field(map, "page_height", DEFAULT_PAGE_HEIGHT),
        margin: float_field(map, "margin", 72.0),
        title: string_field(map, "title", "Niao PDF"),
    }
}

fn extract_opts_from(map: Option<&HashMap<String, ValueRef>>) -> ExtractOpts {
    let pages = map.and_then(|m| {
        m.get("pages").and_then(|v| match &*v.borrow() {
            Value::Array(items) => {
                let mut out = Vec::new();
                for item in items {
                    if let Value::Int(n) = &*item.borrow() {
                        if *n >= 0 {
                            out.push(*n as usize);
                        }
                    }
                }
                if out.is_empty() {
                    None
                } else {
                    Some(out)
                }
            }
            _ => None,
        })
    });
    ExtractOpts {
        pages,
        page_separator: string_field(map, "page_separator", "\n\n"),
    }
}

fn metadata_to_object(meta: &PdfMetadata) -> Value {
    let mut m = HashMap::new();
    if let Some(v) = &meta.title {
        m.insert("title".into(), Value::String(v.clone()).ref_cell());
    }
    if let Some(v) = &meta.author {
        m.insert("author".into(), Value::String(v.clone()).ref_cell());
    }
    if let Some(v) = &meta.subject {
        m.insert("subject".into(), Value::String(v.clone()).ref_cell());
    }
    if let Some(v) = &meta.keywords {
        m.insert("keywords".into(), Value::String(v.clone()).ref_cell());
    }
    if let Some(v) = &meta.creator {
        m.insert("creator".into(), Value::String(v.clone()).ref_cell());
    }
    if let Some(v) = &meta.producer {
        m.insert("producer".into(), Value::String(v.clone()).ref_cell());
    }
    if let Some(v) = &meta.creation_date {
        m.insert("creation_date".into(), Value::String(v.clone()).ref_cell());
    }
    if let Some(v) = &meta.modification_date {
        m.insert("modification_date".into(), Value::String(v.clone()).ref_cell());
    }
    Value::Object(m)
}

fn page_size_to_object(ps: PageSize) -> Value {
    let mut m = HashMap::new();
    m.insert("width".into(), Value::Float(ps.width).ref_cell());
    m.insert("height".into(), Value::Float(ps.height).ref_cell());
    Value::Object(m)
}

// ---------------------------------------------------------------------------
// Read / open
// ---------------------------------------------------------------------------

fn npdf_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "open", span)?;
    let opts = optional_object_arg(args, 1);
    let _ = opts;
    DOCS.with(|store| {
        let mut store = store.borrow_mut();
        match &*args[0].borrow() {
            Value::ByteArray(b) => open_bytes(&mut store, b).map_err(|e| e.message()),
            Value::String(path) => open_file(&mut store, Path::new(path)).map_err(|e| e.message()),
            other => Err(format!(
                "open() expects byte[] or path string, got {}",
                other.type_name()
            )),
        }
        .map(|id| Value::Int(id).ref_cell())
        .map_err(|msg| npdf_err(span, msg))
    })
}

fn npdf_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "close", span)?;
    let id = doc_arg(args, 0, "close", span)?;
    DOCS.with(|store| {
        close_doc(&mut store.borrow_mut(), id)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|_| invalid_handle(span, format!("invalid document handle {id}")))
    })
}

fn npdf_page_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "page_count", span)?;
    let id = doc_arg(args, 0, "page_count", span)?;
    DOCS.with(|store| {
        page_count(&store.borrow(), id)
            .map(|n| Value::Int(n as i64).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_page_size(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "page_size", span)?;
    let id = doc_arg(args, 0, "page_size", span)?;
    let page = if args.len() > 1 {
        int_arg(args, 1, "page_size", span)? as usize
    } else {
        0
    };
    DOCS.with(|store| {
        page_size(&store.borrow(), id, page)
            .map(page_size_to_object)
            .map(|v| v.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_metadata(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "metadata", span)?;
    let id = doc_arg(args, 0, "metadata", span)?;
    DOCS.with(|store| {
        metadata(&store.borrow(), id)
            .map(|m| metadata_to_object(&m).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "save", span)?;
    let id = doc_arg(args, 0, "save", span)?;
    DOCS.with(|store| {
        save_bytes(&store.borrow(), id)
            .map(|b| Value::ByteArray(b).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_write(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "write", span)?;
    let id = doc_arg(args, 0, "write", span)?;
    let path = string_arg(args, 1, "write", span)?;
    DOCS.with(|store| {
        write_file(&store.borrow(), id, Path::new(&path))
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "valid", span)?;
    let bytes = bytes_arg(args, 0, "valid", span)?;
    Ok(Value::Bool(is_valid(&bytes)).ref_cell())
}

fn npdf_rotate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 3, "rotate", span)?;
    let id = doc_arg(args, 0, "rotate", span)?;
    let page = int_arg(args, 1, "rotate", span)? as usize;
    let degrees = int_arg(args, 2, "rotate", span)? as i32;
    DOCS.with(|store| {
        rotate_page(&mut store.borrow_mut(), id, page, degrees)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_remove_pages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "remove_pages", span)?;
    let id = doc_arg(args, 0, "remove_pages", span)?;
    let pages = int_list_arg(args, 1, "remove_pages", span)?;
    DOCS.with(|store| {
        remove_pages(&mut store.borrow_mut(), id, &pages)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_copy_pages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "copy_pages", span)?;
    let id = doc_arg(args, 0, "copy_pages", span)?;
    let pages = int_list_arg(args, 1, "copy_pages", span)?;
    DOCS.with(|store| {
        let mut store = store.borrow_mut();
        copy_pages(&mut store, id, &pages)
            .map(|new_id| Value::Int(new_id).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

// ---------------------------------------------------------------------------
// Extract
// ---------------------------------------------------------------------------

fn npdf_extract_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "extract_text", span)?;
    let opts = extract_opts_from(optional_object_arg(args, 1).as_ref());
    match &*args[0].borrow() {
        Value::Int(id) if *id > 0 => DOCS.with(|store| {
            extract_text_doc(&store.borrow(), *id, &opts)
                .map(|s| Value::String(s).ref_cell())
                .map_err(|e| map_err(span, e))
        }),
        Value::ByteArray(b) => extract_text_bytes(b, &opts)
            .map(|s| Value::String(s).ref_cell())
            .map_err(|e| map_err(span, e)),
        other => Err(type_err(
            span,
            format!("extract_text() expects document handle or byte[], got {}", other.type_name()),
        )),
    }
}

fn npdf_extract_page_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "extract_page_text", span)?;
    let id = doc_arg(args, 0, "extract_page_text", span)?;
    let page = int_arg(args, 1, "extract_page_text", span)? as usize;
    DOCS.with(|store| {
        extract_page_text(&store.borrow(), id, page)
            .map(|s| Value::String(s).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_pages_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "pages_text", span)?;
    let id = doc_arg(args, 0, "pages_text", span)?;
    DOCS.with(|store| {
        pages_text(&store.borrow(), id)
            .map(|items| {
                Value::Array(
                    items
                        .into_iter()
                        .map(|s| Value::String(s).ref_cell())
                        .collect(),
                )
                .ref_cell()
            })
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_extract_pages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "extract_pages", span)?;
    let id = doc_arg(args, 0, "extract_pages", span)?;
    let pages = int_list_arg(args, 1, "extract_pages", span)?;
    DOCS.with(|store| {
        extract_pages_bytes(&store.borrow(), id, &pages)
            .map(|b| Value::ByteArray(b).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_page_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "page_bytes", span)?;
    let id = doc_arg(args, 0, "page_bytes", span)?;
    let page = int_arg(args, 1, "page_bytes", span)? as usize;
    DOCS.with(|store| {
        extract_pages_bytes(&store.borrow(), id, &[page])
            .map(|b| Value::ByteArray(b).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

// ---------------------------------------------------------------------------
// Merge / split
// ---------------------------------------------------------------------------

fn npdf_merge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "merge", span)?;
    let parts = bytes_list_arg(args, 0, "merge", span)?;
    merge_bytes(&parts)
        .map(|b| Value::ByteArray(b).ref_cell())
        .map_err(|e| map_err(span, e))
}

fn npdf_merge_docs(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "merge_docs", span)?;
    let ids = int_list_arg(args, 0, "merge_docs", span)?;
    let ids_i64: Vec<i64> = ids.into_iter().map(|n| n as i64).collect();
    DOCS.with(|store| {
        merge_docs(&store.borrow(), &ids_i64)
            .map(|b| Value::ByteArray(b).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_split(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "split", span)?;
    let id = doc_arg(args, 0, "split", span)?;
    let ranges_val = &*args[1].borrow();
    let ranges = match ranges_val {
        Value::Array(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Array(pair) if pair.len() >= 2 => {
                        let start = match &*pair[0].borrow() {
                            Value::Int(n) if *n >= 0 => *n as usize,
                            other => {
                                return Err(type_err(
                                    span,
                                    format!("split() range {i} start must be int, got {}", other.type_name()),
                                ));
                            }
                        };
                        let end = match &*pair[1].borrow() {
                            Value::Int(n) if *n >= 0 => *n as usize,
                            other => {
                                return Err(type_err(
                                    span,
                                    format!("split() range {i} end must be int, got {}", other.type_name()),
                                ));
                            }
                        };
                        out.push((start, end));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "split() ranges must be [[start, end], …]; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!("split() expects range array, got {}", other.type_name()),
            ));
        }
    };
    DOCS.with(|store| {
        split_ranges(&store.borrow(), id, &ranges)
            .map(|parts| {
                Value::Array(
                    parts
                        .into_iter()
                        .map(|b| Value::ByteArray(b).ref_cell())
                        .collect(),
                )
                .ref_cell()
            })
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_split_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "split_all", span)?;
    let id = doc_arg(args, 0, "split_all", span)?;
    DOCS.with(|store| {
        split_all(&store.borrow(), id)
            .map(|parts| {
                Value::Array(
                    parts
                        .into_iter()
                        .map(|b| Value::ByteArray(b).ref_cell())
                        .collect(),
                )
                .ref_cell()
            })
            .map_err(|e| map_err(span, e))
    })
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

fn npdf_create(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "create", span)?;
    let opts = create_opts_from(optional_object_arg(args, 0).as_ref());
    BUILDERS.with(|store| {
        create_builder(&mut store.borrow_mut(), &opts)
            .map(|id| Value::Int(id).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_close_builder(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "close_builder", span)?;
    let id = builder_arg(args, 0, "close_builder", span)?;
    BUILDERS.with(|store| {
        close_builder(&mut store.borrow_mut(), id)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|_| invalid_handle(span, format!("invalid builder handle {id}")))
    })
}

fn npdf_add_page(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "add_page", span)?;
    let id = builder_arg(args, 0, "add_page", span)?;
    let opts_map = optional_object_arg(args, 1);
    let opts = opts_map.as_ref().map(|m| create_opts_from(Some(m)));
    BUILDERS.with(|store| {
        add_page(&mut store.borrow_mut(), id, opts)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "text", span)?;
    let id = builder_arg(args, 0, "text", span)?;
    let content = string_arg(args, 1, "text", span)?;
    let map = optional_object_arg(args, 2);
    let font_name = string_field(map.as_ref(), "font", "helvetica");
    let opts = TextOpts {
        x: float_field(map.as_ref(), "x", 72.0),
        y: float_field(map.as_ref(), "y", 720.0),
        size: float_field(map.as_ref(), "size", 12.0),
        font: font_choice(&font_name),
        color: rgb_field(map.as_ref(), "color", (0.0, 0.0, 0.0)),
    };
    BUILDERS.with(|store| {
        text(&mut store.borrow_mut(), id, &content, &opts)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_image(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "image", span)?;
    let id = builder_arg(args, 0, "image", span)?;
    let data = bytes_arg(args, 1, "image", span)?;
    let map = optional_object_arg(args, 2);
    let width = map.as_ref().and_then(|m| m.get("width")).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n as f32),
        Value::Float(f) => Some(*f as f32),
        _ => None,
    });
    let height = map.as_ref().and_then(|m| m.get("height")).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n as f32),
        Value::Float(f) => Some(*f as f32),
        _ => None,
    });
    let opts = ImageOpts {
        x: float_field(map.as_ref(), "x", 72.0),
        y: float_field(map.as_ref(), "y", 400.0),
        width,
        height,
        scale: float_field(map.as_ref(), "scale", 1.0),
    };
    BUILDERS.with(|store| {
        image(&mut store.borrow_mut(), id, &data, &opts)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "table", span)?;
    let id = builder_arg(args, 0, "table", span)?;
    let rows = table_rows_arg(args, 1, "table", span)?;
    let map = optional_object_arg(args, 2);
    let col_widths = map.as_ref().and_then(|m| m.get("col_widths")).and_then(|v| match &*v.borrow() {
        Value::Array(items) => {
            let mut w = Vec::new();
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) => w.push(*n as f32),
                    Value::Float(f) => w.push(*f as f32),
                    _ => return None,
                }
            }
            Some(w)
        }
        _ => None,
    });
    let opts = TableOpts {
        x: float_field(map.as_ref(), "x", 72.0),
        y: float_field(map.as_ref(), "y", 600.0),
        col_widths,
        row_height: float_field(map.as_ref(), "row_height", 20.0),
        font_size: float_field(map.as_ref(), "font_size", 10.0),
        header: bool_field(map.as_ref(), "header", true),
        border: bool_field(map.as_ref(), "border", true),
        border_width: float_field(map.as_ref(), "border_width", 0.5),
        padding: float_field(map.as_ref(), "padding", 4.0),
        header_fill: rgb_field(map.as_ref(), "header_fill", (0.9, 0.9, 0.9)),
        header_font: font_choice(&string_field(map.as_ref(), "header_font", "helvetica-bold")),
        body_font: font_choice(&string_field(map.as_ref(), "body_font", "helvetica")),
    };
    BUILDERS.with(|store| {
        table(&mut store.borrow_mut(), id, &rows, &opts)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_line(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 5, 6, "line", span)?;
    let id = builder_arg(args, 0, "line", span)?;
    let x1 = float_arg(args, 1, "line", span)? as f32;
    let y1 = float_arg(args, 2, "line", span)? as f32;
    let x2 = float_arg(args, 3, "line", span)? as f32;
    let y2 = float_arg(args, 4, "line", span)? as f32;
    let map = optional_object_arg(args, 5);
    let opts = LineOpts {
        width: float_field(map.as_ref(), "width", 1.0),
        color: rgb_field(map.as_ref(), "color", (0.0, 0.0, 0.0)),
    };
    BUILDERS.with(|store| {
        line(&mut store.borrow_mut(), id, x1, y1, x2, y2, &opts)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_rect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 5, 6, "rect", span)?;
    let id = builder_arg(args, 0, "rect", span)?;
    let x = float_arg(args, 1, "rect", span)? as f32;
    let y = float_arg(args, 2, "rect", span)? as f32;
    let w = float_arg(args, 3, "rect", span)? as f32;
    let h = float_arg(args, 4, "rect", span)? as f32;
    let map = optional_object_arg(args, 5);
    let fill = map
        .as_ref()
        .and_then(|m| m.get("fill"))
        .map(|_| rgb_field(map.as_ref(), "fill", (0.8, 0.8, 0.8)));
    let stroke = map
        .as_ref()
        .and_then(|m| m.get("stroke"))
        .map(|_| rgb_field(map.as_ref(), "stroke", (0.0, 0.0, 0.0)));
    let opts = RectOpts {
        fill,
        stroke,
        stroke_width: float_field(map.as_ref(), "stroke_width", 1.0),
    };
    BUILDERS.with(|store| {
        rect(&mut store.borrow_mut(), id, x, y, w, h, &opts)
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_finish(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "finish", span)?;
    let id = builder_arg(args, 0, "finish", span)?;
    BUILDERS.with(|store| {
        finish_builder(&mut store.borrow_mut(), id)
            .map(|b| Value::ByteArray(b).ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

fn npdf_write_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 2, "write_new", span)?;
    let id = builder_arg(args, 0, "write_new", span)?;
    let path = string_arg(args, 1, "write_new", span)?;
    BUILDERS.with(|store| {
        write_builder(&mut store.borrow_mut(), id, Path::new(&path))
            .map(|_| Value::Nil.ref_cell())
            .map_err(|e| map_err(span, e))
    })
}

// ---------------------------------------------------------------------------
// Parallel
// ---------------------------------------------------------------------------

fn npdf_parallel_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "parallel_extract", span)?;
    let inputs = bytes_list_arg(args, 0, "parallel_extract", span)?;
    let map = optional_object_arg(args, 1);
    let opts = extract_opts_from(map.as_ref());
    let threads = int_field(map.as_ref(), "threads", available_threads() as i64) as usize;
    parallel_extract_text(&inputs, &opts, threads)
        .map(|items| {
            Value::Array(
                items
                    .into_iter()
                    .map(|s| Value::String(s).ref_cell())
                    .collect(),
            )
            .ref_cell()
        })
        .map_err(|e| map_err(span, e))
}

fn npdf_parallel_merge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "parallel_merge", span)?;
    let groups_val = &*args[0].borrow();
    let map = optional_object_arg(args, 1);
    let threads = int_field(map.as_ref(), "threads", available_threads() as i64) as usize;
    let groups = match groups_val {
        Value::Array(outer) => {
            let mut groups = Vec::with_capacity(outer.len());
            for (i, g) in outer.iter().enumerate() {
                match &*g.borrow() {
                    Value::Array(inner) => {
                        let mut parts = Vec::with_capacity(inner.len());
                        for item in inner {
                            match &*item.borrow() {
                                Value::ByteArray(b) => parts.push(b.clone()),
                                other => {
                                    return Err(type_err(
                                        span,
                                        format!(
                                            "parallel_merge() group {} item must be byte[], got {}",
                                            i + 1,
                                            other.type_name()
                                        ),
                                    ));
                                }
                            }
                        }
                        groups.push(parts);
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "parallel_merge() expects byte[][][]; group {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            groups
        }
        other => {
            return Err(type_err(
                span,
                format!("parallel_merge() expects array of byte[][] groups, got {}", other.type_name()),
            ));
        }
    };
    parallel_merge(&groups, threads)
        .map(|items| {
            Value::Array(
                items
                    .into_iter()
                    .map(|b| Value::ByteArray(b).ref_cell())
                    .collect(),
            )
            .ref_cell()
        })
        .map_err(|e| map_err(span, e))
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("npdf_open", "open", npdf_open),
        ("npdf_close", "close", npdf_close),
        ("npdf_page_count", "page_count", npdf_page_count),
        ("npdf_page_size", "page_size", npdf_page_size),
        ("npdf_metadata", "metadata", npdf_metadata),
        ("npdf_save", "save", npdf_save),
        ("npdf_write", "write", npdf_write),
        ("npdf_valid", "valid", npdf_valid),
        ("npdf_rotate", "rotate", npdf_rotate),
        ("npdf_remove_pages", "remove_pages", npdf_remove_pages),
        ("npdf_copy_pages", "copy_pages", npdf_copy_pages),
        ("npdf_extract_text", "extract_text", npdf_extract_text),
        ("npdf_extract_page_text", "extract_page_text", npdf_extract_page_text),
        ("npdf_pages_text", "pages_text", npdf_pages_text),
        ("npdf_extract_pages", "extract_pages", npdf_extract_pages),
        ("npdf_page_bytes", "page_bytes", npdf_page_bytes),
        ("npdf_merge", "merge", npdf_merge),
        ("npdf_merge_docs", "merge_docs", npdf_merge_docs),
        ("npdf_split", "split", npdf_split),
        ("npdf_split_all", "split_all", npdf_split_all),
        ("npdf_create", "create", npdf_create),
        ("npdf_close_builder", "close_builder", npdf_close_builder),
        ("npdf_add_page", "add_page", npdf_add_page),
        ("npdf_text", "text", npdf_text),
        ("npdf_image", "image", npdf_image),
        ("npdf_table", "table", npdf_table),
        ("npdf_line", "line", npdf_line),
        ("npdf_rect", "rect", npdf_rect),
        ("npdf_finish", "finish", npdf_finish),
        ("npdf_write_new", "write_new", npdf_write_new),
        ("npdf_parallel_extract", "parallel_extract", npdf_parallel_extract),
        ("npdf_parallel_merge", "parallel_merge", npdf_parallel_merge),
    ]
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "npdf";
pub const MODULE_PATHS: &[&str] = &["npdf", "std/npdf"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub const BUILTIN_COUNT: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn create_finish_doctest() {
        let b = npdf_create(&[], span()).unwrap();
        match &*b.borrow() {
            Value::Int(id) => assert!(*id > 0),
            other => panic!("expected builder handle, got {other:?}"),
        }
    }
}
