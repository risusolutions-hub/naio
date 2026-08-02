//! Native ndocstore standard library — embedded JSON document store with
//! queries and secondary indexes (~tinydb subset).
//!
//! Import with `import "ndocstore"` (or `import "std/ndocstore"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_docstore::{DocumentStore, StoreError, UpdateCond};
use niao_errors::codes;
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const E4560_NDOCSTORE_ARITY: u32 = codes::E4560_NDOCSTORE_ARITY;
const E4561_NDOCSTORE_ERROR: u32 = codes::E4561_NDOCSTORE_ERROR;
const E4562_NDOCSTORE_TYPE: u32 = codes::E4562_NDOCSTORE_TYPE;
const E4563_NDOCSTORE_INVALID_HANDLE: u32 = codes::E4563_NDOCSTORE_INVALID_HANDLE;
const E4564_NDOCSTORE_IO: u32 = codes::E4564_NDOCSTORE_IO;

// ---------------------------------------------------------------------------
// Handle model: store handles + table views sharing the same DocumentStore
// ---------------------------------------------------------------------------

struct StoreEntry {
    store: DocumentStore,
}

struct Handle {
    store_id: i64,
    /// When set, ops target this table; otherwise the store's default table.
    table: Option<String>,
}

thread_local! {
    static STORES: RefCell<HashMap<i64, StoreEntry>> = RefCell::new(HashMap::new());
    static HANDLES: RefCell<HashMap<i64, Handle>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn next_id() -> i64 {
    NEXT_ID.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn register_store(store: DocumentStore) -> i64 {
    let store_id = next_id();
    let handle_id = next_id();
    STORES.with(|s| {
        s.borrow_mut().insert(store_id, StoreEntry { store });
    });
    HANDLES.with(|h| {
        h.borrow_mut().insert(
            handle_id,
            Handle {
                store_id,
                table: None,
            },
        );
    });
    handle_id
}

fn register_table_view(store_id: i64, table: String) -> i64 {
    let handle_id = next_id();
    HANDLES.with(|h| {
        h.borrow_mut().insert(
            handle_id,
            Handle {
                store_id,
                table: Some(table),
            },
        );
    });
    handle_id
}

fn resolve_handle(id: i64, span: Span) -> Result<(i64, Option<String>), ValueRef> {
    HANDLES.with(|h| match h.borrow().get(&id) {
        Some(hh) => Ok((hh.store_id, hh.table.clone())),
        None => Err(error_value(
            E4563_NDOCSTORE_INVALID_HANDLE,
            "ndocstore_error",
            format!("invalid or closed ndocstore handle {id}"),
            span,
        )),
    })
}

fn with_store_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut DocumentStore, Option<&str>) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    let (store_id, table) = match resolve_handle(id, span) {
        Ok(v) => v,
        Err(e) => return Ok(Err(e)),
    };
    STORES.with(|stores| {
        let mut stores = stores.borrow_mut();
        match stores.get_mut(&store_id) {
            Some(entry) => Ok(f(&mut entry.store, table.as_deref())),
            None => Ok(Err(error_value(
                E4563_NDOCSTORE_INVALID_HANDLE,
                "ndocstore_error",
                format!("invalid or closed ndocstore store for handle {id}"),
                span,
            ))),
        }
    })
}

fn with_store<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&DocumentStore, Option<&str>) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    let (store_id, table) = match resolve_handle(id, span) {
        Ok(v) => v,
        Err(e) => return Ok(Err(e)),
    };
    STORES.with(|stores| {
        let stores = stores.borrow();
        match stores.get(&store_id) {
            Some(entry) => Ok(f(&entry.store, table.as_deref())),
            None => Ok(Err(error_value(
                E4563_NDOCSTORE_INVALID_HANDLE,
                "ndocstore_error",
                format!("invalid or closed ndocstore store for handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Errors / conversion
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4562_NDOCSTORE_TYPE, msg.into())
}

fn store_err(span: Span, e: StoreError) -> ValueRef {
    let (code, kind) = match &e {
        StoreError::Io(_) => (E4564_NDOCSTORE_IO, "ndocstore_error"),
        _ => (E4561_NDOCSTORE_ERROR, "ndocstore_error"),
    };
    error_value(code, kind, e.to_string(), span)
}

fn value_to_json(v: &Value, span: Span) -> NiaoResult<JsonValue> {
    match v {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(n) => Ok(JsonValue::Number((*n).into())),
        Value::Float(f) => {
            if let Some(n) = JsonNumber::from_f64(*f) {
                Ok(JsonValue::Number(n))
            } else {
                Ok(JsonValue::Null)
            }
        }
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(value_to_json(&item.borrow(), span)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::IntArray(items) => Ok(JsonValue::Array(
            items
                .iter()
                .map(|n| JsonValue::Number((*n).into()))
                .collect(),
        )),
        Value::Object(map) => {
            let mut out = JsonMap::new();
            for (k, vr) in map {
                out.insert(k.clone(), value_to_json(&vr.borrow(), span)?);
            }
            Ok(JsonValue::Object(out))
        }
        Value::BigInt(n) => {
            let s = n.to_string();
            if let Ok(i) = s.parse::<i64>() {
                Ok(JsonValue::Number(i.into()))
            } else {
                Ok(JsonValue::String(s))
            }
        }
        other => Err(type_err(
            span,
            format!(
                "ndocstore values must be nil/bool/number/string/array/object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn json_to_value(v: JsonValue) -> ValueRef {
    match v {
        JsonValue::Null => Value::Nil.ref_cell(),
        JsonValue::Bool(b) => Value::Bool(b).ref_cell(),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i).ref_cell()
            } else if let Some(u) = n.as_u64() {
                if u <= i64::MAX as u64 {
                    Value::Int(u as i64).ref_cell()
                } else {
                    Value::String(u.to_string()).ref_cell()
                }
            } else if let Some(f) = n.as_f64() {
                Value::Float(f).ref_cell()
            } else {
                Value::Nil.ref_cell()
            }
        }
        JsonValue::String(s) => Value::String(s).ref_cell(),
        JsonValue::Array(items) => {
            Value::Array(items.into_iter().map(json_to_value).collect()).ref_cell()
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                out.insert(k, json_to_value(v));
            }
            Value::Object(out).ref_cell()
        }
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4560_NDOCSTORE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4560_NDOCSTORE_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a handle (int) as argument {}, got {}",
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
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn object_json(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<JsonValue> {
    match &*args[idx].borrow() {
        Value::Object(_) => value_to_json(&args[idx].borrow(), span),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn query_json(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<JsonValue> {
    object_json(args, idx, name, span)
}

fn docs_array(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<JsonValue>> {
    match &*args[idx].borrow() {
        Value::Array(items) => items
            .iter()
            .map(|v| match &*v.borrow() {
                Value::Object(_) => value_to_json(&v.borrow(), span),
                other => Err(type_err(
                    span,
                    format!(
                        "{name}() expects an array of objects, got element {}",
                        other.type_name()
                    ),
                )),
            })
            .collect(),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn resolve_cond(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Result<(Option<Vec<u64>>, Option<JsonValue>), ValueRef>> {
    match &*args[idx].borrow() {
        Value::Int(n) => {
            if *n < 0 {
                return Ok(Err(error_value(
                    E4561_NDOCSTORE_ERROR,
                    "ndocstore_error",
                    "document id must be non-negative",
                    span,
                )));
            }
            Ok(Ok((Some(vec![*n as u64]), None)))
        }
        Value::IntArray(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for n in items {
                if *n < 0 {
                    return Ok(Err(error_value(
                        E4561_NDOCSTORE_ERROR,
                        "ndocstore_error",
                        "document id must be non-negative",
                        span,
                    )));
                }
                ids.push(*n as u64);
            }
            Ok(Ok((Some(ids), None)))
        }
        Value::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for v in items {
                match &*v.borrow() {
                    Value::Int(n) if *n >= 0 => ids.push(*n as u64),
                    Value::Int(_) => {
                        return Ok(Err(error_value(
                            E4561_NDOCSTORE_ERROR,
                            "ndocstore_error",
                            "document id must be non-negative",
                            span,
                        )));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() id list must contain ints, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(Ok((Some(ids), None)))
        }
        Value::Object(_) => {
            let q = value_to_json(&args[idx].borrow(), span)?;
            Ok(Ok((None, Some(q))))
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a query object, id, or id array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// ndocstore_open(path) → handle
/// // >>> import "ndocstore"
/// // >>> let h = ndocstore_memory()
/// // >>> ndocstore_insert(h, {name: "Ada"})
/// // => 1
fn ndocstore_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_open", span)?;
    let path = string_arg(args, 0, "ndocstore_open", span)?;
    match DocumentStore::open(PathBuf::from(path)) {
        Ok(store) => Ok(Value::Int(register_store(store)).ref_cell()),
        Err(e) => Ok(store_err(span, e)),
    }
}

/// ndocstore_memory() → handle
/// // >>> let h = ndocstore_memory()
/// // >>> ndocstore_len(h)
/// // => 0
fn ndocstore_memory(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ndocstore_memory", span)?;
    Ok(Value::Int(register_store(DocumentStore::memory())).ref_cell())
}

/// ndocstore_close(h) → bool
/// // >>> let h = ndocstore_memory(); ndocstore_close(h)
/// // => true
fn ndocstore_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_close", span)?;
    let id = handle_arg(args, 0, "ndocstore_close", span)?;
    let info = HANDLES.with(|h| h.borrow_mut().remove(&id));
    let Some(handle) = info else {
        return Ok(Value::Bool(false).ref_cell());
    };
    // Drop store only when no other handles reference it.
    let still = HANDLES.with(|h| {
        h.borrow()
            .values()
            .any(|hh| hh.store_id == handle.store_id)
    });
    if !still {
        // Auto-flush file-backed stores on last close.
        STORES.with(|s| {
            if let Some(mut entry) = s.borrow_mut().remove(&handle.store_id) {
                let _ = entry.store.flush();
            }
        });
    }
    Ok(Value::Bool(true).ref_cell())
}

/// ndocstore_flush(h) → true
/// // >>> let h = ndocstore_memory(); ndocstore_flush(h)
/// // => true
fn ndocstore_flush(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_flush", span)?;
    let id = handle_arg(args, 0, "ndocstore_flush", span)?;
    match with_store_mut(id, span, |store, _| {
        store.flush().map_err(|e| store_err(span, e))?;
        Ok(Value::Bool(true).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_path(h) → string|nil
fn ndocstore_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_path", span)?;
    let id = handle_arg(args, 0, "ndocstore_path", span)?;
    match with_store(id, span, |store, _| {
        Ok(match store.path() {
            Some(p) => Value::String(p.to_string_lossy().into_owned()).ref_cell(),
            None => Value::Nil.ref_cell(),
        })
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_tables(h) → [string]
fn ndocstore_tables(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_tables", span)?;
    let id = handle_arg(args, 0, "ndocstore_tables", span)?;
    match with_store(id, span, |store, _| {
        let items: Vec<ValueRef> = store
            .tables()
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect();
        Ok(Value::Array(items).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_table(h, name) → handle (table view)
/// // >>> let db = ndocstore_memory(); let t = ndocstore_table(db, "users"); ndocstore_insert(t, {n: 1})
/// // => 1
fn ndocstore_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_table", span)?;
    let id = handle_arg(args, 0, "ndocstore_table", span)?;
    let name = string_arg(args, 1, "ndocstore_table", span)?;
    if name == niao_docstore::META_KEY {
        return Ok(error_value(
            E4561_NDOCSTORE_ERROR,
            "ndocstore_error",
            format!("reserved table name '{}'", niao_docstore::META_KEY),
            span,
        ));
    }
    let store_id = match resolve_handle(id, span) {
        Ok((sid, _)) => sid,
        Err(e) => return Ok(e),
    };
    let exists = STORES.with(|s| s.borrow().contains_key(&store_id));
    if !exists {
        return Ok(error_value(
            E4563_NDOCSTORE_INVALID_HANDLE,
            "ndocstore_error",
            format!("invalid or closed ndocstore store for handle {id}"),
            span,
        ));
    }
    STORES.with(|s| {
        if let Some(entry) = s.borrow_mut().get_mut(&store_id) {
            entry.store.ensure_table(&name);
        }
    });
    Ok(Value::Int(register_table_view(store_id, name)).ref_cell())
}

/// ndocstore_drop_table(h, name) → bool
fn ndocstore_drop_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_drop_table", span)?;
    let id = handle_arg(args, 0, "ndocstore_drop_table", span)?;
    let name = string_arg(args, 1, "ndocstore_drop_table", span)?;
    match with_store_mut(id, span, |store, _| {
        store
            .drop_table(&name)
            .map(|b| Value::Bool(b).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_default_table(h) → string
fn ndocstore_default_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_default_table", span)?;
    let id = handle_arg(args, 0, "ndocstore_default_table", span)?;
    match with_store(id, span, |store, table| {
        Ok(Value::String(
            table
                .unwrap_or_else(|| store.default_table())
                .to_string(),
        )
        .ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_set_default_table(h, name) → true
fn ndocstore_set_default_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_set_default_table", span)?;
    let id = handle_arg(args, 0, "ndocstore_set_default_table", span)?;
    let name = string_arg(args, 1, "ndocstore_set_default_table", span)?;
    match with_store_mut(id, span, |store, _| {
        store.set_default_table(&name);
        Ok(Value::Bool(true).ref_cell())
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_insert(h, doc) → int
/// // >>> let h = ndocstore_memory(); ndocstore_insert(h, {name: "Ada", age: 36})
/// // => 1
fn ndocstore_insert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_insert", span)?;
    let id = handle_arg(args, 0, "ndocstore_insert", span)?;
    let doc = object_json(args, 1, "ndocstore_insert", span)?;
    match with_store_mut(id, span, |store, table| {
        store
            .insert(table, doc)
            .map(|doc_id| Value::Int(doc_id as i64).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_insert_many(h, docs) → [int]
fn ndocstore_insert_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_insert_many", span)?;
    let id = handle_arg(args, 0, "ndocstore_insert_many", span)?;
    let docs = docs_array(args, 1, "ndocstore_insert_many", span)?;
    match with_store_mut(id, span, |store, table| {
        store
            .insert_many(table, docs)
            .map(|ids| {
                Value::IntArray(ids.into_iter().map(|n| n as i64).collect()).ref_cell()
            })
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_get(h, id) → doc|nil
/// // >>> let h = ndocstore_memory(); let id = ndocstore_insert(h, {x: 1}); ndocstore_get(h, id).x
/// // => 1
fn ndocstore_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_get", span)?;
    let id = handle_arg(args, 0, "ndocstore_get", span)?;
    let doc_id = match &*args[1].borrow() {
        Value::Int(n) if *n >= 0 => *n as u64,
        Value::Int(_) => {
            return Ok(error_value(
                E4561_NDOCSTORE_ERROR,
                "ndocstore_error",
                "document id must be non-negative",
                span,
            ));
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "ndocstore_get() expects an int id, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    match with_store(id, span, |store, table| {
        store
            .get(table, doc_id)
            .map(|opt| match opt {
                Some(d) => json_to_value(d),
                None => Value::Nil.ref_cell(),
            })
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_all(h) → [docs]
fn ndocstore_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_all", span)?;
    let id = handle_arg(args, 0, "ndocstore_all", span)?;
    match with_store(id, span, |store, table| {
        store
            .all(table)
            .map(|rows| Value::Array(rows.into_iter().map(json_to_value).collect()).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_search(h, query) → [docs]
/// // >>> let h = ndocstore_memory(); ndocstore_insert(h, {age: 36}); ndocstore_search(h, {gt: {age: 30}}).len()
/// // => 1
fn ndocstore_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_search", span)?;
    let id = handle_arg(args, 0, "ndocstore_search", span)?;
    let query = query_json(args, 1, "ndocstore_search", span)?;
    match with_store(id, span, |store, table| {
        store
            .search(table, &query)
            .map(|rows| Value::Array(rows.into_iter().map(json_to_value).collect()).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_update(h, fields, query_or_ids) → int
fn ndocstore_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ndocstore_update", span)?;
    let id = handle_arg(args, 0, "ndocstore_update", span)?;
    let fields = object_json(args, 1, "ndocstore_update", span)?;
    let cond = match resolve_cond(args, 2, "ndocstore_update", span)? {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match with_store_mut(id, span, |store, table| {
        let result = match &cond {
            (Some(ids), None) => store.update(table, &fields, UpdateCond::Ids(ids)),
            (None, Some(q)) => store.update(table, &fields, UpdateCond::Query(q)),
            _ => {
                return Err(error_value(
                    E4561_NDOCSTORE_ERROR,
                    "ndocstore_error",
                    "internal: invalid update condition",
                    span,
                ));
            }
        };
        result
            .map(|n| Value::Int(n as i64).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_upsert(h, fields, query) → int (doc id)
fn ndocstore_upsert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ndocstore_upsert", span)?;
    let id = handle_arg(args, 0, "ndocstore_upsert", span)?;
    let fields = object_json(args, 1, "ndocstore_upsert", span)?;
    let query = query_json(args, 2, "ndocstore_upsert", span)?;
    match with_store_mut(id, span, |store, table| {
        store
            .upsert(table, fields, &query)
            .map(|doc_id| Value::Int(doc_id as i64).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_remove(h, query_or_ids) → int
fn ndocstore_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_remove", span)?;
    let id = handle_arg(args, 0, "ndocstore_remove", span)?;
    let cond = match resolve_cond(args, 1, "ndocstore_remove", span)? {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match with_store_mut(id, span, |store, table| {
        let result = match &cond {
            (Some(ids), None) => store.remove(table, UpdateCond::Ids(ids)),
            (None, Some(q)) => store.remove(table, UpdateCond::Query(q)),
            _ => {
                return Err(error_value(
                    E4561_NDOCSTORE_ERROR,
                    "ndocstore_error",
                    "internal: invalid remove condition",
                    span,
                ));
            }
        };
        result
            .map(|n| Value::Int(n as i64).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_truncate(h) → true
fn ndocstore_truncate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_truncate", span)?;
    let id = handle_arg(args, 0, "ndocstore_truncate", span)?;
    match with_store_mut(id, span, |store, table| {
        store
            .truncate(table)
            .map(|_| Value::Bool(true).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_len(h) → int
/// // >>> let h = ndocstore_memory(); ndocstore_insert(h, {a: 1}); ndocstore_len(h)
/// // => 1
fn ndocstore_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_len", span)?;
    let id = handle_arg(args, 0, "ndocstore_len", span)?;
    match with_store(id, span, |store, table| {
        store
            .len(table)
            .map(|n| Value::Int(n as i64).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_contains(h, query) → bool
fn ndocstore_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_contains", span)?;
    let id = handle_arg(args, 0, "ndocstore_contains", span)?;
    let query = query_json(args, 1, "ndocstore_contains", span)?;
    match with_store(id, span, |store, table| {
        store
            .contains(table, &query)
            .map(|b| Value::Bool(b).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_count(h, query?) → int
fn ndocstore_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndocstore_count", span)?;
    let id = handle_arg(args, 0, "ndocstore_count", span)?;
    let query = if args.len() >= 2 && !matches!(&*args[1].borrow(), Value::Nil) {
        Some(query_json(args, 1, "ndocstore_count", span)?)
    } else {
        None
    };
    match with_store(id, span, |store, table| {
        store
            .count(table, query.as_ref())
            .map(|n| Value::Int(n as i64).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_exists(h, id) → bool
fn ndocstore_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_exists", span)?;
    let id = handle_arg(args, 0, "ndocstore_exists", span)?;
    let doc_id = match &*args[1].borrow() {
        Value::Int(n) if *n >= 0 => *n as u64,
        Value::Int(_) => {
            return Ok(Value::Bool(false).ref_cell());
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "ndocstore_exists() expects an int id, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    match with_store(id, span, |store, table| {
        store
            .exists(table, doc_id)
            .map(|b| Value::Bool(b).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_create_index(h, field) → true
/// // >>> let h = ndocstore_memory(); ndocstore_create_index(h, "age")
/// // => true
fn ndocstore_create_index(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_create_index", span)?;
    let id = handle_arg(args, 0, "ndocstore_create_index", span)?;
    let field = string_arg(args, 1, "ndocstore_create_index", span)?;
    match with_store_mut(id, span, |store, table| {
        store
            .create_index(table, &field)
            .map(|_| Value::Bool(true).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_drop_index(h, field) → bool
fn ndocstore_drop_index(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndocstore_drop_index", span)?;
    let id = handle_arg(args, 0, "ndocstore_drop_index", span)?;
    let field = string_arg(args, 1, "ndocstore_drop_index", span)?;
    match with_store_mut(id, span, |store, table| {
        store
            .drop_index(table, &field)
            .map(|b| Value::Bool(b).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_indexes(h) → [string]
fn ndocstore_indexes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_indexes", span)?;
    let id = handle_arg(args, 0, "ndocstore_indexes", span)?;
    match with_store(id, span, |store, table| {
        store
            .indexes(table)
            .map(|fields| {
                Value::Array(fields.into_iter().map(|s| Value::String(s).ref_cell()).collect())
                    .ref_cell()
            })
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_to_json(h, pretty?) → string
fn ndocstore_to_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndocstore_to_json", span)?;
    let id = handle_arg(args, 0, "ndocstore_to_json", span)?;
    let pretty = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Bool(b) => *b,
            Value::Nil => true,
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ndocstore_to_json() pretty must be bool, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    } else {
        true
    };
    match with_store(id, span, |store, _| {
        store
            .to_json_string(pretty)
            .map(|s| Value::String(s).ref_cell())
            .map_err(|e| store_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// ndocstore_from_json(json) → handle
fn ndocstore_from_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndocstore_from_json", span)?;
    let text = string_arg(args, 0, "ndocstore_from_json", span)?;
    match DocumentStore::from_json(&text) {
        Ok(store) => Ok(Value::Int(register_store(store)).ref_cell()),
        Err(e) => Ok(store_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ndocstore_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndocstore_fns![
    ("ndocstore_open", "open", ndocstore_open),
    ("ndocstore_memory", "memory", ndocstore_memory),
    ("ndocstore_close", "close", ndocstore_close),
    ("ndocstore_flush", "flush", ndocstore_flush),
    ("ndocstore_path", "path", ndocstore_path),
    ("ndocstore_tables", "tables", ndocstore_tables),
    ("ndocstore_table", "table", ndocstore_table),
    ("ndocstore_drop_table", "drop_table", ndocstore_drop_table),
    ("ndocstore_default_table", "default_table", ndocstore_default_table),
    ("ndocstore_set_default_table", "set_default_table", ndocstore_set_default_table),
    ("ndocstore_insert", "insert", ndocstore_insert),
    ("ndocstore_insert_many", "insert_many", ndocstore_insert_many),
    ("ndocstore_get", "get", ndocstore_get),
    ("ndocstore_all", "all", ndocstore_all),
    ("ndocstore_search", "search", ndocstore_search),
    ("ndocstore_update", "update", ndocstore_update),
    ("ndocstore_upsert", "upsert", ndocstore_upsert),
    ("ndocstore_remove", "remove", ndocstore_remove),
    ("ndocstore_truncate", "truncate", ndocstore_truncate),
    ("ndocstore_len", "len", ndocstore_len),
    ("ndocstore_contains", "contains", ndocstore_contains),
    ("ndocstore_count", "count", ndocstore_count),
    ("ndocstore_exists", "exists", ndocstore_exists),
    ("ndocstore_create_index", "create_index", ndocstore_create_index),
    ("ndocstore_drop_index", "drop_index", ndocstore_drop_index),
    ("ndocstore_indexes", "indexes", ndocstore_indexes),
    ("ndocstore_to_json", "to_json", ndocstore_to_json),
    ("ndocstore_from_json", "from_json", ndocstore_from_json),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
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

pub const MODULE_NAME: &str = "ndocstore";
pub const MODULE_PATHS: &[&str] = &["ndocstore", "std/ndocstore"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn memory_insert_search() {
        let h = ndocstore_memory(&[], span()).unwrap();
        let mut doc = HashMap::new();
        doc.insert("name".into(), Value::String("Ada".into()).ref_cell());
        doc.insert("age".into(), Value::Int(36).ref_cell());
        let id = ndocstore_insert(&[h.clone(), Value::Object(doc).ref_cell()], span()).unwrap();
        assert_eq!(*id.borrow(), Value::Int(1));

        let mut q = HashMap::new();
        let mut gt = HashMap::new();
        gt.insert("age".into(), Value::Int(30).ref_cell());
        q.insert("gt".into(), Value::Object(gt).ref_cell());
        let rows = ndocstore_search(&[h.clone(), Value::Object(q).ref_cell()], span()).unwrap();
        match &*rows.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 1),
            other => panic!("expected array, got {other:?}"),
        }
        ndocstore_close(&[h], span()).unwrap();
    }
}
