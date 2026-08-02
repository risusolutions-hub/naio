//! Native `nvec` standard library — in-memory vector database with cosine
//! similarity search (brute-force for N ≤ 256, NSW/HNSW-lite graph for N > 256)
//! plus an optional Qdrant REST backend for production-scale deployments.
//!
//! Import with `import "nvec"` (or `import "std/nvec"`).
//!
//! ## Quick start
//!
//! ```niao
//! import "nvec"
//!
//! // In-memory index (dimension auto-detected on first insert)
//! let idx = nvec.open()
//! nvec.insert(idx, "doc1", [0.1, 0.9, 0.3], {label: "cats"})
//! nvec.insert(idx, "doc2", [0.8, 0.1, 0.5], {label: "dogs"})
//!
//! let hits = nvec.search(idx, [0.15, 0.85, 0.3], 3)
//! // hits = [{id:"doc1", score:0.99, metadata:{label:"cats"}}, ...]
//! nvec.close(idx)
//! ```

mod index;
mod qdrant;

use self::index::{MetaVal, VecIndex};
use self::qdrant::QdrantBackend;
use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Backend discriminant
// ---------------------------------------------------------------------------

enum Backend {
    Memory {
        index: VecIndex,
        /// Optional path for auto-save / initial load.
        save_path: Option<String>,
    },
    Qdrant(QdrantBackend),
}

// ---------------------------------------------------------------------------
// Thread-local handle table
// ---------------------------------------------------------------------------

thread_local! {
    static HANDLES: RefCell<HashMap<i64, Backend>> = RefCell::new(HashMap::new());
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

fn with_backend<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Backend) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|h| {
        let mut guard = h.borrow_mut();
        match guard.get_mut(&id) {
            Some(b) => Ok(Ok(f(b))),
            None => Ok(Err(nvec_err(
                span,
                format!("invalid or closed nvec handle {id}"),
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn nvec_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2791_NVEC_ERROR, "nvec_error", msg.into(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2790_NVEC_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2790_NVEC_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(RuntimeError::TypeError {
            message: format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
            line: span.line,
            col: span.col,
        }),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::TypeError {
            message: format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
            line: span.line,
            col: span.col,
        }),
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

fn opt_float_arg(args: &[ValueRef], idx: usize) -> Option<f64> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Float(f) => Some(*f),
        Value::Int(n) => Some(*n as f64),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Vector and metadata extraction
// ---------------------------------------------------------------------------

fn extract_vector(v: &ValueRef, name: &str, span: Span) -> NiaoResult<Vec<f32>> {
    match &*v.borrow() {
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for elem in arr {
                match &*elem.borrow() {
                    Value::Float(f) => out.push(*f as f32),
                    Value::Int(n) => out.push(*n as f32),
                    other => {
                        return Err(RuntimeError::at(
                            span,
                            codes::E2792_NVEC_TYPE,
                            format!(
                                "{name}() vector elements must be numbers, got {}",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Ok(out)
        }
        other => Err(RuntimeError::at(
            span,
            codes::E2792_NVEC_TYPE,
            format!(
                "{name}() vector must be an array, got {}",
                other.type_name()
            ),
        )),
    }
}

fn value_to_meta(v: &Value) -> MetaVal {
    match v {
        Value::String(s) => MetaVal::Str(s.clone()),
        Value::Int(n) => MetaVal::Int(*n),
        Value::Float(f) => MetaVal::Float(*f),
        Value::Bool(b) => MetaVal::Bool(*b),
        _ => MetaVal::Nil,
    }
}

fn extract_metadata(v: &ValueRef) -> HashMap<String, MetaVal> {
    match &*v.borrow() {
        Value::Object(map) => map
            .iter()
            .map(|(k, vr)| (k.clone(), value_to_meta(&*vr.borrow())))
            .collect(),
        _ => HashMap::new(),
    }
}

fn meta_to_value(m: &MetaVal) -> ValueRef {
    match m {
        MetaVal::Str(s) => Value::String(s.clone()).ref_cell(),
        MetaVal::Int(n) => Value::Int(*n).ref_cell(),
        MetaVal::Float(f) => Value::Float(*f).ref_cell(),
        MetaVal::Bool(b) => Value::Bool(*b).ref_cell(),
        MetaVal::Nil => Value::Nil.ref_cell(),
    }
}

fn hit_to_value(id: &str, score: f32, metadata: &HashMap<String, MetaVal>) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("id".to_string(), Value::String(id.to_string()).ref_cell());
    map.insert("score".to_string(), Value::Float(score as f64).ref_cell());
    let meta_map: HashMap<String, ValueRef> = metadata
        .iter()
        .map(|(k, v)| (k.clone(), meta_to_value(v)))
        .collect();
    map.insert("metadata".to_string(), Value::Object(meta_map).ref_cell());
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// `nvec.open(path?, dim?) -> handle_id`
///
/// Opens (or creates) an in-memory vector index.  
/// - No args: ephemeral index, dimension auto-detected on first insert.  
/// - String arg: persistence path; loaded if the file exists.  
/// - Int arg: fixed dimension.  
/// - String + Int (or Int + String): both.
fn nvec_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "nvec.open", span)?;

    let mut path: Option<String> = None;
    let mut dim: usize = 0;

    for arg in args {
        match &*arg.borrow() {
            Value::String(s) => path = Some(s.clone()),
            Value::Int(n) if *n > 0 => dim = *n as usize,
            Value::Nil => {}
            other => {
                return Ok(nvec_err(
                    span,
                    format!(
                        "nvec.open() unexpected argument type: {}",
                        other.type_name()
                    ),
                ))
            }
        }
    }

    let index = if let Some(ref p) = path {
        if std::path::Path::new(p).exists() {
            match VecIndex::load_from_file(p) {
                Ok(idx) => idx,
                Err(e) => return Ok(nvec_err(span, format!("nvec.open() load failed: {e}"))),
            }
        } else {
            VecIndex::new(dim)
        }
    } else {
        VecIndex::new(dim)
    };

    let id = new_handle();
    HANDLES.with(|h| {
        h.borrow_mut().insert(
            id,
            Backend::Memory {
                index,
                save_path: path,
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

/// `nvec.connect(url, api_key?, collection?) -> handle_id`
///
/// Opens a Qdrant REST backend handle. No network call is made until the first
/// operation.
fn nvec_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nvec.connect", span)?;
    let url = string_arg(args, 0, "nvec.connect", span)?;
    let api_key = opt_string_arg(args, 1);
    let collection = opt_string_arg(args, 2).unwrap_or_else(|| "niao_default".to_string());

    let backend = QdrantBackend::new(url, api_key, collection);
    let id = new_handle();
    HANDLES.with(|h| {
        h.borrow_mut().insert(id, Backend::Qdrant(backend));
    });
    Ok(Value::Int(id).ref_cell())
}

/// `nvec.insert(id, vec_id, vector[], metadata{}) -> true | error`
fn nvec_insert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nvec.insert", span)?;
    let handle_id = int_arg(args, 0, "nvec.insert", span)?;
    let vec_id = string_arg(args, 1, "nvec.insert", span)?;
    let vector = extract_vector(&args[2], "nvec.insert", span)?;
    let metadata = extract_metadata(&args[3]);

    match with_backend(handle_id, span, |b| match b {
        Backend::Memory { index, .. } => index.insert(vec_id.clone(), vector, metadata),
        Backend::Qdrant(q) => q.upsert(&vec_id, &vector, &metadata).and_then(|_| {
            Err("nvec.insert on Qdrant is semantically upsert; use nvec.upsert".to_string())
        }),
    })? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(nvec_err(span, e)),
        Err(e) => Ok(e),
    }
}

/// `nvec.upsert(id, vec_id, vector[], metadata{}) -> true | error`
fn nvec_upsert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nvec.upsert", span)?;
    let handle_id = int_arg(args, 0, "nvec.upsert", span)?;
    let vec_id = string_arg(args, 1, "nvec.upsert", span)?;
    let vector = extract_vector(&args[2], "nvec.upsert", span)?;
    let metadata = extract_metadata(&args[3]);

    match with_backend(handle_id, span, |b| match b {
        Backend::Memory { index, .. } => index.upsert(vec_id.clone(), vector, metadata),
        Backend::Qdrant(q) => q.upsert(&vec_id, &vector, &metadata),
    })? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(nvec_err(span, e)),
        Err(e) => Ok(e),
    }
}

/// `nvec.search(id, query[], top_k?, threshold?) -> hits[]`
///
/// `hits` is an array of `{id, score, metadata{}}` objects sorted by
/// descending cosine similarity.
fn nvec_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nvec.search", span)?;
    let handle_id = int_arg(args, 0, "nvec.search", span)?;
    let query = extract_vector(&args[1], "nvec.search", span)?;
    let top_k = opt_int_arg(args, 2)
        .map(|n| n.max(1) as usize)
        .unwrap_or(10);
    let threshold = opt_float_arg(args, 3).unwrap_or(0.0) as f32;

    match with_backend(handle_id, span, |b| match b {
        Backend::Memory { index, .. } => index.search(&query, top_k, threshold),
        Backend::Qdrant(q) => q.search(&query, top_k, threshold),
    })? {
        Ok(Ok(hits)) => {
            let arr: Vec<ValueRef> = hits
                .iter()
                .map(|h| hit_to_value(&h.id, h.score, &h.metadata))
                .collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Ok(Err(e)) => Ok(nvec_err(span, e)),
        Err(e) => Ok(e),
    }
}

/// `nvec.delete(id, vec_id) -> true | error`
fn nvec_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nvec.delete", span)?;
    let handle_id = int_arg(args, 0, "nvec.delete", span)?;
    let vec_id = string_arg(args, 1, "nvec.delete", span)?;

    match with_backend(handle_id, span, |b| match b {
        Backend::Memory { index, .. } => Ok(index.delete(&vec_id)),
        Backend::Qdrant(q) => q.delete(&vec_id),
    })? {
        Ok(Ok(b)) => Ok(Value::Bool(b).ref_cell()),
        Ok(Err(e)) => Ok(nvec_err(span, e)),
        Err(e) => Ok(e),
    }
}

/// `nvec.count(id) -> int`
fn nvec_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvec.count", span)?;
    let handle_id = int_arg(args, 0, "nvec.count", span)?;

    match with_backend(handle_id, span, |b| match b {
        Backend::Memory { index, .. } => Ok(index.count()),
        Backend::Qdrant(q) => q.count(),
    })? {
        Ok(Ok(n)) => Ok(Value::Int(n as i64).ref_cell()),
        Ok(Err(e)) => Ok(nvec_err(span, e)),
        Err(e) => Ok(e),
    }
}

/// `nvec.save(id, path) -> true | error`
///
/// Saves the in-memory index to a file. No-op (returns `false`) on Qdrant
/// handles (the data lives in the Qdrant server).
fn nvec_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nvec.save", span)?;
    let handle_id = int_arg(args, 0, "nvec.save", span)?;
    let path = string_arg(args, 1, "nvec.save", span)?;

    match with_backend(handle_id, span, |b| match b {
        Backend::Memory { index, save_path } => {
            *save_path = Some(path.clone());
            index.save_to_file(&path)
        }
        Backend::Qdrant(_) => Err("nvec.save() is not applicable to Qdrant handles".to_string()),
    })? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(nvec_err(span, e)),
        Err(e) => Ok(e),
    }
}

/// `nvec.load(path) -> handle_id | error`
///
/// Loads a previously saved in-memory index from a file and returns a new
/// handle.
fn nvec_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvec.load", span)?;
    let path = string_arg(args, 0, "nvec.load", span)?;

    match VecIndex::load_from_file(&path) {
        Ok(index) => {
            let id = new_handle();
            HANDLES.with(|h| {
                h.borrow_mut().insert(
                    id,
                    Backend::Memory {
                        index,
                        save_path: Some(path),
                    },
                );
            });
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(nvec_err(span, format!("nvec.load() failed: {e}"))),
    }
}

/// `nvec.close(id) -> true`
fn nvec_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nvec.close", span)?;
    let handle_id = int_arg(args, 0, "nvec.close", span)?;
    let removed = HANDLES.with(|h| h.borrow_mut().remove(&handle_id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nvec_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nvec_fns![
    ("nvec_open", "open", nvec_open),
    ("nvec_connect", "connect", nvec_connect),
    ("nvec_insert", "insert", nvec_insert),
    ("nvec_upsert", "upsert", nvec_upsert),
    ("nvec_search", "search", nvec_search),
    ("nvec_delete", "delete", nvec_delete),
    ("nvec_count", "count", nvec_count),
    ("nvec_save", "save", nvec_save),
    ("nvec_load", "load", nvec_load),
    ("nvec_close", "close", nvec_close),
];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nvec";
pub const MODULE_PATHS: &[&str] = &["nvec", "std/nvec"];

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
    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    fn vec_val(floats: &[f64]) -> ValueRef {
        Value::Array(floats.iter().map(|&x| f(x)).collect()).ref_cell()
    }

    fn meta_obj(pairs: &[(&str, ValueRef)]) -> ValueRef {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), Rc::clone(v));
        }
        Value::Object(m).ref_cell()
    }

    fn open_handle() -> ValueRef {
        nvec_open(&[], span()).unwrap()
    }

    fn handle_id(v: &ValueRef) -> i64 {
        match &*v.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle int, got {other:?}"),
        }
    }

    #[test]
    fn open_close_roundtrip() {
        let h = open_handle();
        let hid = handle_id(&h);
        let ok = nvec_close(&[h], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));
        // Second close returns false (already removed).
        let ok2 = nvec_close(&[i(hid)], span()).unwrap();
        assert!(matches!(&*ok2.borrow(), Value::Bool(false)));
    }

    #[test]
    fn insert_search_basic() {
        let h = open_handle();
        nvec_upsert(
            &[h.clone(), s("a"), vec_val(&[1.0, 0.0, 0.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        nvec_upsert(
            &[h.clone(), s("b"), vec_val(&[0.0, 1.0, 0.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        nvec_upsert(
            &[h.clone(), s("c"), vec_val(&[0.9, 0.1, 0.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        let hits = nvec_search(&[h.clone(), vec_val(&[1.0, 0.0, 0.0]), i(3)], span()).unwrap();
        let arr = match &*hits.borrow() {
            Value::Array(a) => a.clone(),
            other => panic!("expected array, got {other:?}"),
        };
        assert!(!arr.is_empty());
        let first_id = match &*arr[0].borrow() {
            Value::Object(map) => match &*map.get("id").unwrap().borrow() {
                Value::String(s) => s.clone(),
                _ => panic!(),
            },
            _ => panic!(),
        };
        assert_eq!(first_id, "a");
        nvec_close(&[h], span()).unwrap();
    }

    #[test]
    fn count_reflects_insertions_and_deletions() {
        let h = open_handle();
        nvec_upsert(
            &[h.clone(), s("x"), vec_val(&[1.0, 0.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        nvec_upsert(
            &[h.clone(), s("y"), vec_val(&[0.0, 1.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        let n = nvec_count(&[h.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(2)));
        nvec_delete(&[h.clone(), s("x")], span()).unwrap();
        let n2 = nvec_count(&[h.clone()], span()).unwrap();
        assert!(matches!(&*n2.borrow(), Value::Int(1)));
        nvec_close(&[h], span()).unwrap();
    }

    #[test]
    fn invalid_handle_returns_error_value() {
        let v = nvec_count(&[i(999_999)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn search_threshold_filtering() {
        let h = open_handle();
        nvec_upsert(
            &[h.clone(), s("a"), vec_val(&[1.0, 0.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        nvec_upsert(
            &[h.clone(), s("b"), vec_val(&[0.0, 1.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        // Threshold 0.9 — only "a" should pass.
        let hits = nvec_search(&[h.clone(), vec_val(&[1.0, 0.0]), i(5), f(0.9)], span()).unwrap();
        let arr = match &*hits.borrow() {
            Value::Array(a) => a.clone(),
            _ => panic!(),
        };
        assert_eq!(arr.len(), 1);
        nvec_close(&[h], span()).unwrap();
    }

    #[test]
    fn metadata_preserved_in_search_hits() {
        let h = open_handle();
        let meta = meta_obj(&[("category", s("fruit")), ("priority", i(3))]);
        nvec_upsert(&[h.clone(), s("apple"), vec_val(&[1.0, 0.0]), meta], span()).unwrap();
        let hits = nvec_search(&[h.clone(), vec_val(&[1.0, 0.0])], span()).unwrap();
        let arr = match &*hits.borrow() {
            Value::Array(a) => a.clone(),
            _ => panic!(),
        };
        let first = match &*arr[0].borrow() {
            Value::Object(m) => m.clone(),
            _ => panic!(),
        };
        let meta_obj_ref = first.get("metadata").unwrap().clone();
        let meta_map = match &*meta_obj_ref.borrow() {
            Value::Object(m) => m.clone(),
            _ => panic!(),
        };
        assert!(
            matches!(&*meta_map.get("category").unwrap().borrow(), Value::String(s) if s == "fruit")
        );
        assert!(matches!(
            &*meta_map.get("priority").unwrap().borrow(),
            Value::Int(3)
        ));
        nvec_close(&[h], span()).unwrap();
    }

    #[test]
    fn save_and_load() {
        let tmp = std::env::temp_dir().join("nvec_mod_test.nvecd");
        let path = tmp.to_str().unwrap().to_string();

        let h = open_handle();
        nvec_upsert(
            &[h.clone(), s("v1"), vec_val(&[1.0, 0.0, 0.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        nvec_upsert(
            &[h.clone(), s("v2"), vec_val(&[0.0, 1.0, 0.0]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        let save_ok = nvec_save(&[h.clone(), s(&path)], span()).unwrap();
        assert!(matches!(&*save_ok.borrow(), Value::Bool(true)));
        nvec_close(&[h], span()).unwrap();

        let h2 = nvec_load(&[s(&path)], span()).unwrap();
        assert!(!matches!(&*h2.borrow(), Value::Error(_)));
        let n = nvec_count(&[h2.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(2)));
        nvec_close(&[h2], span()).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_with_path_loads_existing() {
        let tmp = std::env::temp_dir().join("nvec_open_path_test.nvecd");
        let path = tmp.to_str().unwrap().to_string();

        // First: create and save.
        let h = open_handle();
        nvec_upsert(
            &[h.clone(), s("doc"), vec_val(&[0.5, 0.5]), meta_obj(&[])],
            span(),
        )
        .unwrap();
        nvec_save(&[h.clone(), s(&path)], span()).unwrap();
        nvec_close(&[h], span()).unwrap();

        // Second: open with path — should auto-load.
        let h2 = nvec_open(&[s(&path)], span()).unwrap();
        let n = nvec_count(&[h2.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(1)));
        nvec_close(&[h2], span()).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn upsert_overwrites_existing() {
        let h = open_handle();
        nvec_upsert(
            &[
                h.clone(),
                s("k"),
                vec_val(&[1.0, 0.0]),
                meta_obj(&[("v", i(1))]),
            ],
            span(),
        )
        .unwrap();
        nvec_upsert(
            &[
                h.clone(),
                s("k"),
                vec_val(&[0.0, 1.0]),
                meta_obj(&[("v", i(2))]),
            ],
            span(),
        )
        .unwrap();
        let n = nvec_count(&[h.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(1)));
        let hits = nvec_search(&[h.clone(), vec_val(&[0.0, 1.0])], span()).unwrap();
        let arr = match &*hits.borrow() {
            Value::Array(a) => a.clone(),
            _ => panic!(),
        };
        // Should find the updated vector.
        let obj = match &*arr[0].borrow() {
            Value::Object(m) => m.clone(),
            _ => panic!(),
        };
        assert!(matches!(&*obj.get("id").unwrap().borrow(), Value::String(s) if s == "k"));
        nvec_close(&[h], span()).unwrap();
    }
}
