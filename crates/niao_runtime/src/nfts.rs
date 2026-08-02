//! Native `nfts` standard library — embedded full-text search with inverted
//! index, BM25 ranking, phrase/prefix queries, and facets (~whoosh;
//! tantivy-class; pairs with `nvec` for hybrid keyword+vector RAG).
//!
//! Import with `import "nfts"` (or `import "std/nfts"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_fts::{analyze, FacetCount, Hit, Index};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4140: u32 = codes::E4140_NFTS_ARITY;
const E4141: u32 = codes::E4141_NFTS_ERROR;
const E4142: u32 = codes::E4142_NFTS_TYPE;
const E4143: u32 = codes::E4143_NFTS_INVALID_HANDLE;

thread_local! {
    static HANDLES: RefCell<HashMap<i64, Index>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn nfts_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4141, "nfts_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4143,
        "nfts_error",
        format!("invalid or closed nfts handle {id}"),
        span,
    )
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4140,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4140,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4142, msg.into())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_string_arg(args: &[ValueRef], idx: usize) -> Option<String> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn opt_int_arg(args: &[ValueRef], idx: usize) -> Option<i64> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n),
        _ => None,
    })
}

fn string_map_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, String>> {
    match &*args[idx].borrow() {
        Value::Object(map) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                match &*v.borrow() {
                    Value::String(s) => {
                        out.insert(k.clone(), s.clone());
                    }
                    Value::Int(n) => {
                        out.insert(k.clone(), n.to_string());
                    }
                    Value::Float(f) => {
                        out.insert(k.clone(), f.to_string());
                    }
                    Value::Bool(b) => {
                        out.insert(k.clone(), b.to_string());
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() field/facet values must be scalar, got {}",
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
                "{name}() expects object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_string_map_arg(args: &[ValueRef], idx: usize) -> HashMap<String, String> {
    args.get(idx)
        .and_then(|v| match &*v.borrow() {
            Value::Object(map) => {
                let mut out = HashMap::new();
                for (k, v) in map {
                    match &*v.borrow() {
                        Value::String(s) => {
                            out.insert(k.clone(), s.clone());
                        }
                        Value::Int(n) => {
                            out.insert(k.clone(), n.to_string());
                        }
                        Value::Float(f) => {
                            out.insert(k.clone(), f.to_string());
                        }
                        Value::Bool(b) => {
                            out.insert(k.clone(), b.to_string());
                        }
                        _ => {}
                    }
                }
                Some(out)
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn str_map_to_value(m: &HashMap<String, String>) -> ValueRef {
    let mut obj = HashMap::new();
    for (k, v) in m {
        obj.insert(k.clone(), Value::String(v.clone()).ref_cell());
    }
    Value::Object(obj).ref_cell()
}

fn hit_to_value(h: &Hit) -> ValueRef {
    let mut obj = HashMap::new();
    obj.insert("id".to_string(), Value::String(h.id.clone()).ref_cell());
    obj.insert("score".to_string(), Value::Float(h.score).ref_cell());
    obj.insert("fields".to_string(), str_map_to_value(&h.fields));
    obj.insert("facets".to_string(), str_map_to_value(&h.facets));
    Value::Object(obj).ref_cell()
}

fn facet_to_value(f: &FacetCount) -> ValueRef {
    let mut obj = HashMap::new();
    obj.insert("value".to_string(), Value::String(f.value.clone()).ref_cell());
    obj.insert("count".to_string(), Value::Int(f.count as i64).ref_cell());
    Value::Object(obj).ref_cell()
}

fn with_index<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Index) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|h| {
        let mut guard = h.borrow_mut();
        match guard.get_mut(&id) {
            Some(idx) => Ok(Ok(f(idx))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> type(idx)
/// // => "int"
fn nfts_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nfts.open", span)?;
    let path = opt_string_arg(args, 0);
    match Index::open_or_create(path.as_deref()) {
        Ok(idx) => {
            let id = new_handle();
            HANDLES.with(|h| h.borrow_mut().insert(id, idx));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(nfts_err(span, e.message)),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.close(idx)
/// // => true
fn nfts_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfts.close", span)?;
    let id = int_arg(args, 0, "nfts.close", span)?;
    let removed = HANDLES.with(|h| h.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "d1", {body: "hello world"})
/// // => true
fn nfts_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nfts.add", span)?;
    let id = int_arg(args, 0, "nfts.add", span)?;
    let doc_id = string_arg(args, 1, "nfts.add", span)?;
    let fields = string_map_arg(args, 2, "nfts.add", span)?;
    let facets = opt_string_map_arg(args, 3);
    match with_index(id, span, |idx| idx.add(&doc_id, fields, facets))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(nfts_err(span, e.message)),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.update(idx, "d1", {body: "updated text"})
/// // => true
fn nfts_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nfts.update", span)?;
    let id = int_arg(args, 0, "nfts.update", span)?;
    let doc_id = string_arg(args, 1, "nfts.update", span)?;
    let fields = string_map_arg(args, 2, "nfts.update", span)?;
    let facets = opt_string_map_arg(args, 3);
    match with_index(id, span, |idx| {
        idx.update(&doc_id, fields, facets);
        true
    })? {
        Ok(_) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "d1", {body: "x"})
/// // >>> nfts.delete(idx, "d1")
/// // => true
fn nfts_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfts.delete", span)?;
    let id = int_arg(args, 0, "nfts.delete", span)?;
    let doc_id = string_arg(args, 1, "nfts.delete", span)?;
    match with_index(id, span, |idx| idx.delete(&doc_id))? {
        Ok(ok) => Ok(Value::Bool(ok).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "d1", {body: "hello"})
/// // >>> nfts.get(idx, "d1").id
/// // => "d1"
fn nfts_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfts.get", span)?;
    let id = int_arg(args, 0, "nfts.get", span)?;
    let doc_id = string_arg(args, 1, "nfts.get", span)?;
    match with_index(id, span, |idx| idx.get_fields(&doc_id))? {
        Ok(Some((fields, facets))) => {
            let mut obj = HashMap::new();
            obj.insert("id".to_string(), Value::String(doc_id).ref_cell());
            obj.insert("fields".to_string(), str_map_to_value(&fields));
            obj.insert("facets".to_string(), str_map_to_value(&facets));
            Ok(Value::Object(obj).ref_cell())
        }
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.count(idx)
/// // => 0
fn nfts_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfts.count", span)?;
    let id = int_arg(args, 0, "nfts.count", span)?;
    match with_index(id, span, |idx| idx.count())? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "1", {body: "quick brown fox"})
/// // >>> nfts.search(idx, "brown", 5)[0].id
/// // => "1"
fn nfts_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nfts.search", span)?;
    let id = int_arg(args, 0, "nfts.search", span)?;
    let query = string_arg(args, 1, "nfts.search", span)?;
    let top_k = opt_int_arg(args, 2).unwrap_or(10).max(0) as usize;
    let default_field = match args.get(3) {
        Some(v) => match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            Value::Object(map) => map
                .get("field")
                .and_then(|fv| match &*fv.borrow() {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                }),
            _ => None,
        },
        None => None,
    };
    match with_index(id, span, |idx| {
        idx.search(&query, top_k, default_field.as_deref())
    })? {
        Ok(hits) => Ok(Value::Array(hits.iter().map(hit_to_value).collect()).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "1", {body: "catalog category"})
/// // >>> len(nfts.suggest(idx, "cat", "body", 5)) > 0
/// // => true
fn nfts_suggest(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nfts.suggest", span)?;
    let id = int_arg(args, 0, "nfts.suggest", span)?;
    let prefix = string_arg(args, 1, "nfts.suggest", span)?;
    let mut field: Option<String> = None;
    let mut limit: usize = 10;
    if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::String(s) => {
                field = Some(s.clone());
                limit = opt_int_arg(args, 3).unwrap_or(10).max(0) as usize;
            }
            Value::Int(n) => {
                limit = (*n).max(0) as usize;
            }
            _ => {}
        }
    }
    match with_index(id, span, |idx| idx.suggest(&prefix, field.as_deref(), limit))? {
        Ok(terms) => Ok(Value::Array(
            terms
                .into_iter()
                .map(|t| Value::String(t).ref_cell())
                .collect(),
        )
        .ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "1", {body: "apple"}, {color: "red"})
/// // >>> nfts.facets(idx, "color")[0].value
/// // => "red"
fn nfts_facets(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nfts.facets", span)?;
    let id = int_arg(args, 0, "nfts.facets", span)?;
    let facet_field = string_arg(args, 1, "nfts.facets", span)?;
    let mut query: Option<String> = None;
    let mut limit: usize = 20;
    if args.len() >= 3 {
        match &*args[2].borrow() {
            Value::String(s) => query = Some(s.clone()),
            Value::Int(n) => limit = (*n).max(0) as usize,
            _ => {}
        }
    }
    if args.len() >= 4 {
        if let Some(n) = opt_int_arg(args, 3) {
            limit = n.max(0) as usize;
        }
    }
    match with_index(id, span, |idx| {
        idx.facets(&facet_field, query.as_deref(), limit)
    })? {
        Ok(counts) => Ok(Value::Array(counts.iter().map(facet_to_value).collect()).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "1", {body: "x"})
/// // >>> nfts.schema(idx).fields[0]
/// // => "body"
fn nfts_schema(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfts.schema", span)?;
    let id = int_arg(args, 0, "nfts.schema", span)?;
    match with_index(id, span, |idx| idx.schema())? {
        Ok(s) => {
            let mut obj = HashMap::new();
            obj.insert(
                "fields".to_string(),
                Value::Array(
                    s.fields
                        .into_iter()
                        .map(|f| Value::String(f).ref_cell())
                        .collect(),
                )
                .ref_cell(),
            );
            obj.insert(
                "facet_fields".to_string(),
                Value::Array(
                    s.facet_fields
                        .into_iter()
                        .map(|f| Value::String(f).ref_cell())
                        .collect(),
                )
                .ref_cell(),
            );
            Ok(Value::Object(obj).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "1", {body: "persist me"})
/// // >>> nfts.save(idx, "nfts_doctest_tmp.nfts")
/// // => true
fn nfts_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nfts.save", span)?;
    let id = int_arg(args, 0, "nfts.save", span)?;
    let path = string_arg(args, 1, "nfts.save", span)?;
    match with_index(id, span, |idx| idx.save(&path))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(nfts_err(span, e.message)),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "1", {body: "persist me"})
/// // >>> nfts.save(idx, "nfts_doctest_load.nfts")
/// // >>> let idx2 = nfts.load("nfts_doctest_load.nfts")
/// // >>> nfts.count(idx2)
/// // => 1
fn nfts_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfts.load", span)?;
    let path = string_arg(args, 0, "nfts.load", span)?;
    match Index::load(&path) {
        Ok(idx) => {
            let id = new_handle();
            HANDLES.with(|h| h.borrow_mut().insert(id, idx));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(nfts_err(span, e.message)),
    }
}

/// // >>> import "nfts"
/// // >>> let idx = nfts.open()
/// // >>> nfts.add(idx, "1", {body: "x"})
/// // >>> nfts.clear(idx)
/// // >>> nfts.count(idx)
/// // => 0
fn nfts_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfts.clear", span)?;
    let id = int_arg(args, 0, "nfts.clear", span)?;
    match with_index(id, span, |idx| {
        idx.clear();
        true
    })? {
        Ok(_) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// // >>> import "nfts"
/// // >>> nfts.analyze("Hello, World!")
/// // => ["hello", "world"]
fn nfts_analyze(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nfts.analyze", span)?;
    let text = string_arg(args, 0, "nfts.analyze", span)?;
    let tokens = analyze(&text);
    Ok(Value::Array(
        tokens
            .into_iter()
            .map(|t| Value::String(t).ref_cell())
            .collect(),
    )
    .ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nfts_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nfts_fns![
    ("nfts_open", "open", nfts_open),
    ("nfts_close", "close", nfts_close),
    ("nfts_add", "add", nfts_add),
    ("nfts_update", "update", nfts_update),
    ("nfts_delete", "delete", nfts_delete),
    ("nfts_get", "get", nfts_get),
    ("nfts_count", "count", nfts_count),
    ("nfts_search", "search", nfts_search),
    ("nfts_suggest", "suggest", nfts_suggest),
    ("nfts_facets", "facets", nfts_facets),
    ("nfts_schema", "schema", nfts_schema),
    ("nfts_save", "save", nfts_save),
    ("nfts_load", "load", nfts_load),
    ("nfts_clear", "clear", nfts_clear),
    ("nfts_analyze", "analyze", nfts_analyze),
];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nfts";
pub const MODULE_PATHS: &[&str] = &["nfts", "std/nfts"];

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }
    fn i(n: i64) -> ValueRef {
        Value::Int(n).ref_cell()
    }
    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }
    fn obj(pairs: &[(&str, &str)]) -> ValueRef {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), s(v));
        }
        Value::Object(m).ref_cell()
    }
    fn handle(v: ValueRef) -> ValueRef {
        match &*v.borrow() {
            Value::Int(_) => v,
            other => panic!("expected handle, got {other:?}"),
        }
    }

    #[test]
    fn open_add_search_close() {
        let h = handle(nfts_open(&[], span()).unwrap());
        nfts_add(
            &[h.clone(), s("1"), obj(&[("body", "quick brown fox")])],
            span(),
        )
        .unwrap();
        nfts_add(
            &[h.clone(), s("2"), obj(&[("body", "lazy dog")])],
            span(),
        )
        .unwrap();
        let hits = nfts_search(&[h.clone(), s("brown"), i(5)], span()).unwrap();
        match &*hits.borrow() {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 1);
                match &*arr[0].borrow() {
                    Value::Object(m) => match &*m.get("id").unwrap().borrow() {
                        Value::String(s) => assert_eq!(s, "1"),
                        other => panic!("expected id string, got {other:?}"),
                    },
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &*nfts_close(&[h], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
    }

    #[test]
    fn phrase_prefix_facets() {
        let h = handle(nfts_open(&[], span()).unwrap());
        nfts_add(
            &[
                h.clone(),
                s("a"),
                obj(&[("body", "new york city")]),
                obj(&[("region", "east")]),
            ],
            span(),
        )
        .unwrap();
        nfts_add(
            &[
                h.clone(),
                s("b"),
                obj(&[("body", "york new city")]),
                obj(&[("region", "west")]),
            ],
            span(),
        )
        .unwrap();
        let phrase = nfts_search(&[h.clone(), s(r#""new york""#), i(10)], span()).unwrap();
        match &*phrase.borrow() {
            Value::Array(arr) => assert_eq!(arr.len(), 1),
            other => panic!("{other:?}"),
        }
        nfts_add(
            &[h.clone(), s("c"), obj(&[("body", "catalog items")])],
            span(),
        )
        .unwrap();
        let sug = nfts_suggest(&[h.clone(), s("cat"), s("body"), i(10)], span()).unwrap();
        match &*sug.borrow() {
            Value::Array(arr) => assert!(!arr.is_empty()),
            other => panic!("{other:?}"),
        }
        let fac = nfts_facets(&[h.clone(), s("region")], span()).unwrap();
        match &*fac.borrow() {
            Value::Array(arr) => assert_eq!(arr.len(), 2),
            other => panic!("{other:?}"),
        }
        nfts_close(&[h], span()).unwrap();
    }

    #[test]
    fn duplicate_add_and_invalid_handle() {
        let h = handle(nfts_open(&[], span()).unwrap());
        nfts_add(&[h.clone(), s("x"), obj(&[("body", "one")])], span()).unwrap();
        let err = nfts_add(&[h.clone(), s("x"), obj(&[("body", "two")])], span()).unwrap();
        assert!(matches!(&*err.borrow(), Value::Error(_)));
        let bad = nfts_count(&[i(999_999)], span()).unwrap();
        assert!(matches!(&*bad.borrow(), Value::Error(_)));
        nfts_close(&[h], span()).unwrap();
    }

    #[test]
    fn analyze_tokens() {
        let v = nfts_analyze(&[s("Hello, World!")], span()).unwrap();
        match &*v.borrow() {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 2);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nfts_rt_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.nfts");
        let path_s = path.to_str().unwrap();
        let h = handle(nfts_open(&[], span()).unwrap());
        nfts_update(
            &[
                h.clone(),
                s("d1"),
                obj(&[("body", "persisted document")]),
                obj(&[("lang", "en")]),
            ],
            span(),
        )
        .unwrap();
        let ok = nfts_save(&[h.clone(), s(path_s)], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));
        nfts_close(&[h], span()).unwrap();
        let h2 = handle(nfts_load(&[s(path_s)], span()).unwrap());
        let c = nfts_count(&[h2.clone()], span()).unwrap();
        assert!(matches!(&*c.borrow(), Value::Int(1)));
        nfts_close(&[h2], span()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arity_error() {
        let err = nfts_count(&[], span());
        assert!(err.is_err());
    }
}
