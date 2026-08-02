//! Native `nkv` standard library — embedded ordered key-value store (ACID,
//! prefix scans, MVCC snapshots). Backed by [`niao_kv`] / redb.
//!
//! Import with `import "nkv"` (or `import "std/nkv"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_kv::{KvError, ScanOptions, ScanPair, Store, Txn, DEFAULT_TABLE};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Value codec (typed binary)
// ---------------------------------------------------------------------------

const TAG_NIL: u8 = 0;
const TAG_BOOL: u8 = 1;
const TAG_I64: u8 = 2;
const TAG_F64: u8 = 3;
const TAG_STR: u8 = 4;
const TAG_BYTES: u8 = 5;

fn encode_value(v: &Value, span: Span) -> NiaoResult<Vec<u8>> {
    match v {
        Value::Nil => Ok(vec![TAG_NIL]),
        Value::Bool(b) => Ok(vec![TAG_BOOL, u8::from(*b)]),
        Value::Int(n) => {
            let mut buf = vec![TAG_I64];
            buf.extend_from_slice(&n.to_le_bytes());
            Ok(buf)
        }
        Value::Float(f) => {
            let mut buf = vec![TAG_F64];
            buf.extend_from_slice(&f.to_le_bytes());
            Ok(buf)
        }
        Value::String(s) => {
            let bytes = s.as_bytes();
            let mut buf = vec![TAG_STR];
            buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(bytes);
            Ok(buf)
        }
        Value::ByteArray(b) => {
            let mut buf = vec![TAG_BYTES];
            buf.extend_from_slice(&(b.len() as u32).to_le_bytes());
            buf.extend_from_slice(b);
            Ok(buf)
        }
        other => Err(type_err(
            span,
            format!(
                "nkv value must be nil, bool, int, float, string, or bytes; got {}",
                other.type_name()
            ),
        )),
    }
}

fn decode_value(bytes: &[u8], span: Span) -> NiaoResult<ValueRef> {
    let Some(&tag) = bytes.first() else {
        return Ok(nkv_err(span, "empty value encoding"));
    };
    let rest = &bytes[1..];
    match tag {
        TAG_NIL => Ok(Value::Nil.ref_cell()),
        TAG_BOOL => {
            let b = match rest.first() {
                Some(0) => false,
                Some(1) => true,
                _ => return Ok(nkv_err(span, "invalid bool encoding")),
            };
            Ok(Value::Bool(b).ref_cell())
        }
        TAG_I64 => {
            if rest.len() != 8 {
                return Ok(nkv_err(span, "invalid int encoding"));
            }
            let mut arr = [0u8; 8];
            arr.copy_from_slice(rest);
            Ok(Value::Int(i64::from_le_bytes(arr)).ref_cell())
        }
        TAG_F64 => {
            if rest.len() != 8 {
                return Ok(nkv_err(span, "invalid float encoding"));
            }
            let mut arr = [0u8; 8];
            arr.copy_from_slice(rest);
            Ok(Value::Float(f64::from_le_bytes(arr)).ref_cell())
        }
        TAG_STR | TAG_BYTES => {
            if rest.len() < 4 {
                return Ok(nkv_err(span, "invalid length prefix"));
            }
            let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
            if rest.len() < 4 + len {
                return Ok(nkv_err(span, "truncated value payload"));
            }
            let payload = &rest[4..4 + len];
            if tag == TAG_STR {
                match std::str::from_utf8(payload) {
                    Ok(s) => Ok(Value::String(s.to_string()).ref_cell()),
                    Err(_) => Ok(nkv_err(span, "string tag with invalid UTF-8")),
                }
            } else {
                Ok(Value::ByteArray(payload.to_vec()).ref_cell())
            }
        }
        _ => Ok(nkv_err(span, format!("unknown value tag {tag}"))),
    }
}

fn bytes_to_key(bytes: Vec<u8>) -> ValueRef {
    match std::str::from_utf8(&bytes) {
        Ok(s) => Value::String(s.to_string()).ref_cell(),
        Err(_) => Value::ByteArray(bytes).ref_cell(),
    }
}

fn key_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or bytes as key at argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn scan_bytes_arg(v: &ValueRef, field: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*v.borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!("{field} must be string or bytes, got {}", other.type_name()),
        )),
    }
}

fn scan_pair_object(pair: ScanPair, span: Span) -> NiaoResult<ValueRef> {
    let mut map = HashMap::new();
    map.insert("key".to_string(), bytes_to_key(pair.key));
    map.insert("value".to_string(), decode_value(&pair.value, span)?);
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Handle table
// ---------------------------------------------------------------------------

enum Handle {
    Db(Store),
    Txn { db_id: i64, txn: RefCell<Txn> },
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, Handle>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_handle() -> i64 {
    NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn insert_db(store: Store) -> i64 {
    let id = alloc_handle();
    HANDLES.with(|h| h.borrow_mut().insert(id, Handle::Db(store)));
    id
}

fn insert_txn(db_id: i64, txn: Txn) -> i64 {
    let id = alloc_handle();
    HANDLES.with(|h| {
        h.borrow_mut().insert(
            id,
            Handle::Txn {
                db_id,
                txn: RefCell::new(txn),
            },
        )
    });
    id
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4582_NKV_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E4580_NKV_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4580_NKV_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nkv_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4581_NKV_ERROR, "nkv_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        codes::E4583_NKV_INVALID_HANDLE,
        "nkv_error",
        format!("invalid or closed handle {id}"),
        span,
    )
}

fn txn_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4584_NKV_TXN, "nkv_error", msg.into(), span)
}

fn map_kv_err(span: Span, r: Result<impl Sized, KvError>) -> Result<(), ValueRef> {
    r.map(|_| ()).map_err(|e| kv_to_value_err(span, e))
}

fn kv_to_value_err(span: Span, e: KvError) -> ValueRef {
    match e {
        KvError::ReadOnly | KvError::TxnClosed => txn_err(span, e.to_string()),
        KvError::Store(m) | KvError::Invalid(m) => nkv_err(span, m),
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

fn bool_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<bool> {
    match &*args[idx].borrow() {
        Value::Bool(b) => Ok(*b),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a bool as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_bool(obj: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    obj.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(default)
}

fn table_from_args(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    if args.len() > idx {
        string_arg(args, idx, name, span)
    } else {
        Ok(DEFAULT_TABLE.to_string())
    }
}

fn parse_open_opts(opts: &ValueRef, span: Span) -> NiaoResult<bool> {
    match &*opts.borrow() {
        Value::Nil => Ok(true),
        Value::Object(map) => Ok(optional_bool(map, "create", true)),
        other => Err(type_err(
            span,
            format!("opts must be object, got {}", other.type_name()),
        )),
    }
}

fn parse_scan_opts(opts: &ValueRef, span: Span) -> NiaoResult<(ScanOptions, String)> {
    let mut out = ScanOptions::default();
    let mut table = DEFAULT_TABLE.to_string();
    match &*opts.borrow() {
        Value::Nil => {}
        Value::Object(map) => {
            if let Some(v) = map.get("table") {
                table = match &*v.borrow() {
                    Value::String(s) => s.clone(),
                    other => {
                        return Err(type_err(
                            span,
                            format!("opts.table must be string, got {}", other.type_name()),
                        ));
                    }
                };
            }
            if let Some(v) = map.get("prefix") {
                out.prefix = Some(scan_bytes_arg(v, "opts.prefix", span)?);
            }
            if let Some(v) = map.get("start") {
                out.start = Some(scan_bytes_arg(v, "opts.start", span)?);
            }
            if let Some(v) = map.get("end") {
                out.end = Some(scan_bytes_arg(v, "opts.end", span)?);
            }
            out.end_inclusive = optional_bool(map, "end_inclusive", false);
            if let Some(v) = map.get("limit") {
                match &*v.borrow() {
                    Value::Int(n) if *n >= 0 => out.limit = Some(*n as usize),
                    Value::Nil => out.limit = None,
                    other => {
                        return Err(type_err(
                            span,
                            format!("opts.limit must be non-negative int, got {}", other.type_name()),
                        ));
                    }
                }
            }
            out.reverse = optional_bool(map, "reverse", false);
        }
        other => {
            return Err(type_err(
                span,
                format!("opts must be object, got {}", other.type_name()),
            ));
        }
    }
    Ok((out, table))
}

fn parse_pairs(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<(Vec<u8>, Vec<u8>)>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Object(map) => {
                        let key_v = map.get("key").ok_or_else(|| {
                            type_err(span, format!("{name}() pair {i}: missing key field"))
                        })?;
                        let val_v = map.get("value").ok_or_else(|| {
                            type_err(span, format!("{name}() pair {i}: missing value field"))
                        })?;
                        let key = match &*key_v.borrow() {
                            Value::String(s) => s.as_bytes().to_vec(),
                            Value::ByteArray(b) => b.clone(),
                            other => {
                                return Err(type_err(
                                    span,
                                    format!(
                                        "{name}() pair {i} key must be string or bytes, got {}",
                                        other.type_name()
                                    ),
                                ));
                            }
                        };
                        let value = encode_value(&val_v.borrow(), span)?;
                        out.push((key, value));
                    }
                    Value::Array(pair) if pair.len() == 2 => {
                        let key = match &*pair[0].borrow() {
                            Value::String(s) => s.as_bytes().to_vec(),
                            Value::ByteArray(b) => b.clone(),
                            other => {
                                return Err(type_err(
                                    span,
                                    format!(
                                        "{name}() pair {i} key must be string or bytes, got {}",
                                        other.type_name()
                                    ),
                                ));
                            }
                        };
                        let value = encode_value(&pair[1].borrow(), span)?;
                        out.push((key, value));
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() pair {i} must be {{key,value}} or [k,v], got {}",
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
            format!("{name}() expects array of pairs, got {}", other.type_name()),
        )),
    }
}

fn parse_keys_array(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<Vec<u8>>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.as_bytes().to_vec()),
                    Value::ByteArray(b) => out.push(b.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() key {i} must be string or bytes, got {}",
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
            format!("{name}() expects array of keys, got {}", other.type_name()),
        )),
    }
}

fn with_db<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Store) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|handles| {
        let handles = handles.borrow();
        match handles.get(&id) {
            Some(Handle::Db(store)) => Ok(f(store)),
            Some(Handle::Txn { .. }) => Ok(Err(txn_err(
                span,
                "operation requires a database handle, not a transaction handle",
            ))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

fn with_kv<T>(
    id: i64,
    span: Span,
    f_db: impl FnOnce(&Store) -> Result<T, KvError>,
    f_txn: impl FnOnce(&mut Txn) -> Result<T, KvError>,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|handles| {
        let mut handles = handles.borrow_mut();
        match handles.get_mut(&id) {
            Some(Handle::Db(store)) => Ok(f_db(store).map_err(|e| kv_to_value_err(span, e))),
            Some(Handle::Txn { txn, .. }) => {
                Ok(f_txn(&mut txn.borrow_mut()).map_err(|e| kv_to_value_err(span, e)))
            }
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

fn with_txn<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Txn) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|handles| {
        let mut handles = handles.borrow_mut();
        match handles.get_mut(&id) {
            Some(Handle::Txn { txn, .. }) => Ok(f(&mut txn.borrow_mut())),
            Some(Handle::Db(_)) => Ok(Err(txn_err(
                span,
                "operation requires a transaction handle, not a database handle",
            ))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// nkv_open(path, opts?) → database handle
///
/// >>> import "nkv"
/// >>> type(nkv.open) == "function"
/// => true
fn nkv_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_open", span)?;
    let path = string_arg(args, 0, "nkv_open", span)?;
    let create = if args.len() == 2 {
        parse_open_opts(&args[1], span)?
    } else {
        true
    };
    match Store::open(&path, create) {
        Ok(store) => Ok(Value::Int(insert_db(store)).ref_cell()),
        Err(e) => Ok(kv_to_value_err(span, e)),
    }
}

/// nkv_memory() → in-memory database handle
///
/// >>> let db = nkv.memory()
/// >>> nkv.put(db, "k", "v")
/// => nil
fn nkv_memory(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nkv_memory", span)?;
    match Store::memory() {
        Ok(store) => Ok(Value::Int(insert_db(store)).ref_cell()),
        Err(e) => Ok(kv_to_value_err(span, e)),
    }
}

/// nkv_close(handle) → bool
///
/// >>> nkv.close(db)
/// => true
fn nkv_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_close", span)?;
    let id = int_arg(args, 0, "nkv_close", span)?;
    let removed = HANDLES.with(|h| h.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// nkv_path(db) → string | nil
///
/// >>> nkv.path(nkv.memory())
/// => nil
fn nkv_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_path", span)?;
    let id = int_arg(args, 0, "nkv_path", span)?;
    match with_db(id, span, |store| {
        Ok(match store.path() {
            Some(p) => Value::String(p.display().to_string()).ref_cell(),
            None => Value::Nil.ref_cell(),
        })
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// KV operations
// ---------------------------------------------------------------------------

/// nkv_put(h, key, value, table?) → nil
///
/// >>> let db = nkv.memory()
/// >>> nkv.put(db, "a", 1)
/// => nil
fn nkv_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nkv_put", span)?;
    let id = int_arg(args, 0, "nkv_put", span)?;
    let key = key_arg(args, 1, "nkv_put", span)?;
    let value = encode_value(&args[2].borrow(), span)?;
    let table = table_from_args(args, 3, "nkv_put", span)?;
    match with_kv(
        id,
        span,
        |store| store.put(&table, &key, &value),
        |txn| txn.put(&table, &key, &value),
    )? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_get(h, key, table?) → value | nil
///
/// >>> let db = nkv.memory()
/// >>> nkv.put(db, "x", "hi")
/// >>> nkv.get(db, "x")
/// => "hi"
fn nkv_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nkv_get", span)?;
    let id = int_arg(args, 0, "nkv_get", span)?;
    let key = key_arg(args, 1, "nkv_get", span)?;
    let table = table_from_args(args, 2, "nkv_get", span)?;
    match with_kv(
        id,
        span,
        |store| store.get(&table, &key),
        |txn| txn.get(&table, &key),
    )? {
        Ok(Some(bytes)) => decode_value(&bytes, span),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_get_or(h, key, default, table?) → value
///
/// >>> let db = nkv.memory()
/// >>> nkv.get_or(db, "missing", 0)
/// => 0
fn nkv_get_or(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nkv_get_or", span)?;
    let id = int_arg(args, 0, "nkv_get_or", span)?;
    let key = key_arg(args, 1, "nkv_get_or", span)?;
    let default = Rc::clone(&args[2]);
    let table = table_from_args(args, 3, "nkv_get_or", span)?;
    match with_kv(
        id,
        span,
        |store| store.get(&table, &key),
        |txn| txn.get(&table, &key),
    )? {
        Ok(Some(bytes)) => decode_value(&bytes, span),
        Ok(None) => Ok(default),
        Err(e) => Ok(e),
    }
}

/// nkv_has(h, key, table?) → bool
///
/// >>> let db = nkv.memory()
/// >>> nkv.has(db, "k")
/// => false
fn nkv_has(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nkv_has", span)?;
    let id = int_arg(args, 0, "nkv_has", span)?;
    let key = key_arg(args, 1, "nkv_has", span)?;
    let table = table_from_args(args, 2, "nkv_has", span)?;
    match with_kv(
        id,
        span,
        |store| store.has(&table, &key),
        |txn| txn.has(&table, &key),
    )? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_remove(h, key, table?) → bool
///
/// >>> let db = nkv.memory()
/// >>> nkv.remove(db, "k")
/// => false
fn nkv_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nkv_remove", span)?;
    let id = int_arg(args, 0, "nkv_remove", span)?;
    let key = key_arg(args, 1, "nkv_remove", span)?;
    let table = table_from_args(args, 2, "nkv_remove", span)?;
    match with_kv(
        id,
        span,
        |store| store.remove(&table, &key),
        |txn| txn.remove(&table, &key),
    )? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_clear(h, table?) → int
///
/// >>> let db = nkv.memory()
/// >>> nkv.clear(db)
/// => 0
fn nkv_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_clear", span)?;
    let id = int_arg(args, 0, "nkv_clear", span)?;
    let table = table_from_args(args, 1, "nkv_clear", span)?;
    match with_kv(
        id,
        span,
        |store| store.clear(&table),
        |txn| txn.clear(&table),
    )? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_len(h, table?) → int
///
/// >>> let db = nkv.memory()
/// >>> nkv.len(db)
/// => 0
fn nkv_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_len", span)?;
    let id = int_arg(args, 0, "nkv_len", span)?;
    let table = table_from_args(args, 1, "nkv_len", span)?;
    match with_kv(
        id,
        span,
        |store| store.len(&table),
        |txn| txn.len(&table),
    )? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

/// nkv_begin(db, mode?) → txn handle
///
/// >>> let db = nkv.memory()
/// >>> let tx = nkv.begin(db)
/// >>> type(tx) == "int"
/// => true
fn nkv_begin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_begin", span)?;
    let db_id = int_arg(args, 0, "nkv_begin", span)?;
    let mode = if args.len() == 2 {
        match string_arg(args, 1, "nkv_begin", span)?.as_str() {
            "read" => "read",
            "write" => "write",
            other => {
                return Err(type_err(
                    span,
                    format!("nkv_begin() mode must be \"read\" or \"write\", got {other}"),
                ));
            }
        }
    } else {
        "write"
    };
    match with_db(db_id, span, |store| {
        let txn = if mode == "read" {
            store.begin_read()
        } else {
            store.begin_write()
        };
        match txn {
            Ok(t) => Ok(Value::Int(insert_txn(db_id, t)).ref_cell()),
            Err(e) => Err(kv_to_value_err(span, e)),
        }
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// nkv_snapshot(db) → read txn handle
///
/// >>> let db = nkv.memory()
/// >>> let snap = nkv.snapshot(db)
/// >>> type(snap) == "int"
/// => true
fn nkv_snapshot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_snapshot", span)?;
    let db_id = int_arg(args, 0, "nkv_snapshot", span)?;
    match with_db(db_id, span, |store| {
        match store.begin_read() {
            Ok(txn) => Ok(Value::Int(insert_txn(db_id, txn)).ref_cell()),
            Err(e) => Err(kv_to_value_err(span, e)),
        }
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// nkv_commit(txn) → true
///
/// >>> let db = nkv.memory()
/// >>> let tx = nkv.begin(db)
/// >>> nkv.put(tx, "k", 1)
/// >>> nkv.commit(tx)
/// => true
fn nkv_commit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_commit", span)?;
    let id = int_arg(args, 0, "nkv_commit", span)?;
    match with_txn(id, span, |txn| map_kv_err(span, txn.commit()).map(|_| Value::Bool(true).ref_cell()))? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// nkv_abort(txn) → true
///
/// >>> let db = nkv.memory()
/// >>> let tx = nkv.begin(db)
/// >>> nkv.abort(tx)
/// => true
fn nkv_abort(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_abort", span)?;
    let id = int_arg(args, 0, "nkv_abort", span)?;
    match with_txn(id, span, |txn| map_kv_err(span, txn.abort()).map(|_| Value::Bool(true).ref_cell()))? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// nkv_rollback(txn) → true — alias for `abort`.
fn nkv_rollback(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    nkv_abort(args, span)
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

fn scan_pairs(id: i64, table: &str, opts: &ScanOptions, span: Span) -> NiaoResult<Result<Vec<ScanPair>, ValueRef>> {
    with_kv(
        id,
        span,
        |store| store.scan(table, opts),
        |txn| txn.scan(table, opts),
    )
}

/// nkv_scan(h, opts?) → [{key, value}, ...]
///
/// >>> let db = nkv.memory()
/// >>> nkv.scan(db)
/// => []
fn nkv_scan(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_scan", span)?;
    let id = int_arg(args, 0, "nkv_scan", span)?;
    let (opts, table) = if args.len() == 2 {
        parse_scan_opts(&args[1], span)?
    } else {
        (ScanOptions::default(), DEFAULT_TABLE.to_string())
    };
    match scan_pairs(id, &table, &opts, span)? {
        Ok(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for p in pairs {
                out.push(scan_pair_object(p, span)?);
            }
            Ok(Value::Array(out).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

/// nkv_keys(h, opts?) → [key, ...]
///
/// >>> let db = nkv.memory()
/// >>> nkv.keys(db)
/// => []
fn nkv_keys(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_keys", span)?;
    let id = int_arg(args, 0, "nkv_keys", span)?;
    let (opts, table) = if args.len() == 2 {
        parse_scan_opts(&args[1], span)?
    } else {
        (ScanOptions::default(), DEFAULT_TABLE.to_string())
    };
    match scan_pairs(id, &table, &opts, span)? {
        Ok(pairs) => Ok(Value::Array(pairs.into_iter().map(|p| bytes_to_key(p.key)).collect()).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_values(h, opts?) → [value, ...]
///
/// >>> let db = nkv.memory()
/// >>> nkv.values(db)
/// => []
fn nkv_values(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_values", span)?;
    let id = int_arg(args, 0, "nkv_values", span)?;
    let (opts, table) = if args.len() == 2 {
        parse_scan_opts(&args[1], span)?
    } else {
        (ScanOptions::default(), DEFAULT_TABLE.to_string())
    };
    match scan_pairs(id, &table, &opts, span)? {
        Ok(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for p in pairs {
                out.push(decode_value(&p.value, span)?);
            }
            Ok(Value::Array(out).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

/// nkv_first(h, table?) → {key, value} | nil
///
/// >>> let db = nkv.memory()
/// >>> nkv.first(db)
/// => nil
fn nkv_first(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_first", span)?;
    let id = int_arg(args, 0, "nkv_first", span)?;
    let table = table_from_args(args, 1, "nkv_first", span)?;
    match with_kv(
        id,
        span,
        |store| store.first(&table),
        |txn| txn.first(&table),
    )? {
        Ok(Some(pair)) => scan_pair_object(pair, span),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_last(h, table?) → {key, value} | nil
///
/// >>> let db = nkv.memory()
/// >>> nkv.last(db)
/// => nil
fn nkv_last(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nkv_last", span)?;
    let id = int_arg(args, 0, "nkv_last", span)?;
    let table = table_from_args(args, 1, "nkv_last", span)?;
    match with_kv(
        id,
        span,
        |store| store.last(&table),
        |txn| txn.last(&table),
    )? {
        Ok(Some(pair)) => scan_pair_object(pair, span),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Bulk / tables / misc
// ---------------------------------------------------------------------------

/// nkv_put_many(h, pairs, table?) → int
///
/// >>> let db = nkv.memory()
/// >>> nkv.put_many(db, [["a", 1], ["b", 2]])
/// => 2
fn nkv_put_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nkv_put_many", span)?;
    let id = int_arg(args, 0, "nkv_put_many", span)?;
    let pairs = parse_pairs(args, 1, "nkv_put_many", span)?;
    let table = table_from_args(args, 2, "nkv_put_many", span)?;
    match with_kv(
        id,
        span,
        |store| store.put_many(&table, &pairs),
        |txn| txn.put_many(&table, &pairs),
    )? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nkv_get_many(h, keys, table?) → array
///
/// >>> let db = nkv.memory()
/// >>> nkv.get_many(db, ["a"])
/// => [nil]
fn nkv_get_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nkv_get_many", span)?;
    let id = int_arg(args, 0, "nkv_get_many", span)?;
    let keys = parse_keys_array(args, 1, "nkv_get_many", span)?;
    let table = table_from_args(args, 2, "nkv_get_many", span)?;
    match with_kv(
        id,
        span,
        |store| store.get_many(&table, &keys),
        |txn| txn.get_many(&table, &keys),
    )? {
        Ok(values) => {
            let mut out = Vec::with_capacity(values.len());
            for v in values {
                match v {
                    Some(bytes) => out.push(decode_value(&bytes, span)?),
                    None => out.push(Value::Nil.ref_cell()),
                }
            }
            Ok(Value::Array(out).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

/// nkv_tables(db) → [string]
///
/// >>> let db = nkv.memory()
/// >>> nkv.tables(db)
/// => []
fn nkv_tables(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_tables", span)?;
    let id = int_arg(args, 0, "nkv_tables", span)?;
    match with_db(id, span, |store| {
        store
            .list_tables()
            .map(|names| {
                Value::Array(
                    names
                        .into_iter()
                        .map(|s| Value::String(s).ref_cell())
                        .collect(),
                )
                .ref_cell()
            })
            .map_err(|e| kv_to_value_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// nkv_drop_table(db, name) → bool
///
/// >>> let db = nkv.memory()
/// >>> nkv.drop_table(db, "other")
/// => false
fn nkv_drop_table(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkv_drop_table", span)?;
    let id = int_arg(args, 0, "nkv_drop_table", span)?;
    let name = string_arg(args, 1, "nkv_drop_table", span)?;
    match with_db(id, span, |store| {
        store
            .drop_table(&name)
            .map(|b| Value::Bool(b).ref_cell())
            .map_err(|e| kv_to_value_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// nkv_sync(db) → true
///
/// >>> let db = nkv.memory()
/// >>> nkv.sync(db)
/// => true
fn nkv_sync(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_sync", span)?;
    let id = int_arg(args, 0, "nkv_sync", span)?;
    match with_db(id, span, |store| map_kv_err(span, store.sync()).map(|_| Value::Bool(true).ref_cell()))? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// nkv_stats(db) → object
///
/// >>> let db = nkv.memory()
/// >>> type(nkv.stats(db)) == "object"
/// => true
fn nkv_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkv_stats", span)?;
    let id = int_arg(args, 0, "nkv_stats", span)?;
    match with_db(id, span, |store| {
        store
            .stats()
            .map(|s| {
                let mut map = HashMap::new();
                map.insert("tree_height".to_string(), Value::Int(s.tree_height as i64).ref_cell());
                map.insert(
                    "allocated_pages".to_string(),
                    Value::Int(s.allocated_pages as i64).ref_cell(),
                );
                map.insert("leaf_pages".to_string(), Value::Int(s.leaf_pages as i64).ref_cell());
                map.insert(
                    "branch_pages".to_string(),
                    Value::Int(s.branch_pages as i64).ref_cell(),
                );
                map.insert("stored_bytes".to_string(), Value::Int(s.stored_bytes as i64).ref_cell());
                map.insert(
                    "metadata_bytes".to_string(),
                    Value::Int(s.metadata_bytes as i64).ref_cell(),
                );
                map.insert(
                    "fragmented_bytes".to_string(),
                    Value::Int(s.fragmented_bytes as i64).ref_cell(),
                );
                map.insert("page_size".to_string(), Value::Int(s.page_size as i64).ref_cell());
                Value::Object(map).ref_cell()
            })
            .map_err(|e| kv_to_value_err(span, e))
    })? {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nkv_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nkv_fns![
    ("nkv_open", "open", nkv_open),
    ("nkv_memory", "memory", nkv_memory),
    ("nkv_close", "close", nkv_close),
    ("nkv_path", "path", nkv_path),
    ("nkv_put", "put", nkv_put),
    ("nkv_get", "get", nkv_get),
    ("nkv_get_or", "get_or", nkv_get_or),
    ("nkv_has", "has", nkv_has),
    ("nkv_remove", "remove", nkv_remove),
    ("nkv_clear", "clear", nkv_clear),
    ("nkv_len", "len", nkv_len),
    ("nkv_begin", "begin", nkv_begin),
    ("nkv_snapshot", "snapshot", nkv_snapshot),
    ("nkv_commit", "commit", nkv_commit),
    ("nkv_abort", "abort", nkv_abort),
    ("nkv_rollback", "rollback", nkv_rollback),
    ("nkv_scan", "scan", nkv_scan),
    ("nkv_keys", "keys", nkv_keys),
    ("nkv_values", "values", nkv_values),
    ("nkv_first", "first", nkv_first),
    ("nkv_last", "last", nkv_last),
    ("nkv_put_many", "put_many", nkv_put_many),
    ("nkv_get_many", "get_many", nkv_get_many),
    ("nkv_tables", "tables", nkv_tables),
    ("nkv_drop_table", "drop_table", nkv_drop_table),
    ("nkv_sync", "sync", nkv_sync),
    ("nkv_stats", "stats", nkv_stats),
];

pub const MODULE_NAME: &str = "nkv";
pub const MODULE_PATHS: &[&str] = &["nkv", "std/nkv"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert(
        "DEFAULT_TABLE".to_string(),
        Value::String(DEFAULT_TABLE.to_string()).ref_cell(),
    );
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn b(v: bool) -> ValueRef {
        Value::Bool(v).ref_cell()
    }

    fn f(v: f64) -> ValueRef {
        Value::Float(v).ref_cell()
    }

    fn bytes(v: &[u8]) -> ValueRef {
        Value::ByteArray(v.to_vec()).ref_cell()
    }

    fn db_handle(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected db handle int, got {other:?}"),
        }
    }

    fn assert_nil(v: &ValueRef) {
        assert!(matches!(&*v.borrow(), Value::Nil));
    }

    fn assert_int(v: &ValueRef, expected: i64) {
        match &*v.borrow() {
            Value::Int(n) => assert_eq!(*n, expected),
            other => panic!("expected int {expected}, got {other:?}"),
        }
    }

    fn assert_str(v: &ValueRef, expected: &str) {
        match &*v.borrow() {
            Value::String(s) => assert_eq!(s, expected),
            other => panic!("expected string {expected:?}, got {other:?}"),
        }
    }

    fn assert_bool(v: &ValueRef, expected: bool) {
        match &*v.borrow() {
            Value::Bool(b) => assert_eq!(*b, expected),
            other => panic!("expected bool {expected}, got {other:?}"),
        }
    }

    #[test]
    fn put_get_roundtrip() {
        let db = db_handle(nkv_memory(&[], span()));
        nkv_put(&[i(db), s("hello"), s("world")], span()).unwrap();
        let v = nkv_get(&[i(db), s("hello")], span()).unwrap();
        assert_str(&v, "world");
        nkv_close(&[i(db)], span()).unwrap();
    }

    #[test]
    fn prefix_scan() {
        let db = db_handle(nkv_memory(&[], span()));
        nkv_put(&[i(db), s("user:1"), s("alice")], span()).unwrap();
        nkv_put(&[i(db), s("user:2"), s("bob")], span()).unwrap();
        nkv_put(&[i(db), s("z"), s("other")], span()).unwrap();
        let opts = Value::Object({
            let mut m = HashMap::new();
            m.insert("prefix".to_string(), s("user:"));
            m
        })
        .ref_cell();
        let rows = nkv_scan(&[i(db), opts], span()).unwrap();
        match &*rows.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
        nkv_close(&[i(db)], span()).unwrap();
    }

    #[test]
    fn snapshot_isolation() {
        let db = db_handle(nkv_memory(&[], span()));
        nkv_put(&[i(db), s("k"), s("v1")], span()).unwrap();
        let snap = db_handle(nkv_snapshot(&[i(db)], span()));
        nkv_put(&[i(db), s("k"), s("v2")], span()).unwrap();
        let v_snap = nkv_get(&[i(snap), s("k")], span()).unwrap();
        assert_str(&v_snap, "v1");
        let v_db = nkv_get(&[i(db), s("k")], span()).unwrap();
        assert_str(&v_db, "v2");
        nkv_close(&[i(snap)], span()).unwrap();
        nkv_close(&[i(db)], span()).unwrap();
    }

    #[test]
    fn abort() {
        let db = db_handle(nkv_memory(&[], span()));
        let tx = db_handle(nkv_begin(&[i(db)], span()));
        nkv_put(&[i(tx), s("x"), s("1")], span()).unwrap();
        let ok = nkv_abort(&[i(tx)], span()).unwrap();
        assert_bool(&ok, true);
        let v = nkv_get(&[i(db), s("x")], span()).unwrap();
        assert_nil(&v);
        nkv_close(&[i(db)], span()).unwrap();
    }

    #[test]
    fn invalid_handle_error() {
        let v = nkv_get(&[i(424_242), s("k")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn typed_values() {
        let db = db_handle(nkv_memory(&[], span()));
        nkv_put(&[i(db), s("nil"), Value::Nil.ref_cell()], span()).unwrap();
        nkv_put(&[i(db), s("bool"), b(true)], span()).unwrap();
        nkv_put(&[i(db), s("int"), i(42)], span()).unwrap();
        nkv_put(&[i(db), s("float"), f(3.5)], span()).unwrap();
        nkv_put(&[i(db), s("str"), s("text")], span()).unwrap();
        nkv_put(&[i(db), s("bin"), bytes(&[0, 255])], span()).unwrap();

        assert_nil(&nkv_get(&[i(db), s("nil")], span()).unwrap());
        assert_bool(&nkv_get(&[i(db), s("bool")], span()).unwrap(), true);
        assert_int(&nkv_get(&[i(db), s("int")], span()).unwrap(), 42);
        match &*nkv_get(&[i(db), s("float")], span()).unwrap().borrow() {
            Value::Float(x) => assert!((*x - 3.5).abs() < f64::EPSILON),
            other => panic!("expected float, got {other:?}"),
        }
        assert_str(&nkv_get(&[i(db), s("str")], span()).unwrap(), "text");
        match &*nkv_get(&[i(db), s("bin")], span()).unwrap().borrow() {
            Value::ByteArray(b) => assert_eq!(b, &[0, 255]),
            other => panic!("expected bytes, got {other:?}"),
        }

        let bad = nkv_put(
            &[i(db), s("bad"), Value::Array(vec![]).ref_cell()],
            span(),
        )
        .unwrap_err();
        assert!(matches!(
            bad,
            RuntimeError::Generic {
                code: codes::E4582_NKV_TYPE,
                ..
            }
        ));

        nkv_close(&[i(db)], span()).unwrap();
    }
}
