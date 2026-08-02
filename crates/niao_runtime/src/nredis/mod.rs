//! Native `nredis` standard library — Redis client via RESP2.
//!
//! Wraps `niao_db::redis::Client` (zero-dep RESP2 TCP client).
//! Import with `import "nredis"` or `import "std/nredis"`.
//!
//! All network/command errors are returned as recoverable `Error` values
//! (code E2781). Arity and type errors are hard `RuntimeError`s (E2780/E2782).

mod common;
mod handles;

use crate::{error_from_runtime, error_value, NativeFn, NiaoResult, Value, ValueRef};
use common::*;
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[inline]
fn nredis_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2781_NREDIS_ERROR, "nredis", msg.into(), span)
}

#[inline]
fn ok_true() -> ValueRef {
    Value::Bool(true).ref_cell()
}

#[inline]
fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

/// Convert a raw RESP value to a Niao `Value`.
fn resp_to_niao(v: niao_db::resp::Value) -> Value {
    match v {
        niao_db::resp::Value::SimpleString(s) => Value::String(s),
        niao_db::resp::Value::Error(e) => Value::String(e),
        niao_db::resp::Value::Integer(n) => Value::Int(n),
        niao_db::resp::Value::BulkString(Some(b)) => {
            Value::String(String::from_utf8_lossy(&b).into_owned())
        }
        niao_db::resp::Value::BulkString(None) | niao_db::resp::Value::Null => Value::Nil,
        niao_db::resp::Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|i| resp_to_niao(i).ref_cell())
                .collect(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// `nredis_connect(url) -> handle_id`
fn nredis_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nredis_connect", span)?;
    let url = string_arg(args, 0, "nredis_connect", span)?;
    match niao_db::redis::Client::open(&url) {
        Ok(client) => Ok(Value::Int(handles::alloc(client) as i64).ref_cell()),
        Err(e) => Ok(nredis_error(span, e.to_string())),
    }
}

/// `nredis_ping(id) -> "PONG"`
fn nredis_ping(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nredis_ping", span)?;
    let id = handle_arg(args, 0, "nredis_ping", span)?;
    handles::with_client_mut(id, "nredis_ping", span, |c| {
        c.ping().map_err(|e| e.to_string())
    })
    .map(|s| Value::String(s).ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_get(id, key) -> string | nil`
fn nredis_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nredis_get", span)?;
    let id = handle_arg(args, 0, "nredis_get", span)?;
    let key = string_arg(args, 1, "nredis_get", span)?;
    handles::with_client_mut(id, "nredis_get", span, |c| {
        c.get(&key).map_err(|e| e.to_string())
    })
    .map(|opt| match opt {
        Some(s) => Value::String(s).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_set(id, key, value) -> true`
fn nredis_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nredis_set", span)?;
    let id = handle_arg(args, 0, "nredis_set", span)?;
    let key = string_arg(args, 1, "nredis_set", span)?;
    let val = string_arg(args, 2, "nredis_set", span)?;
    handles::with_client_mut(id, "nredis_set", span, |c| {
        c.set(&key, &val).map_err(|e| e.to_string())
    })
    .map(|_| ok_true())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_del(id, key) -> true`
fn nredis_del(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nredis_del", span)?;
    let id = handle_arg(args, 0, "nredis_del", span)?;
    let key = string_arg(args, 1, "nredis_del", span)?;
    handles::with_client_mut(id, "nredis_del", span, |c| {
        c.del(&key).map_err(|e| e.to_string())
    })
    .map(|_| ok_true())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_incr(id, key, by?) -> int`
///
/// Increments `key` by `by` (default 1). Uses `INCRBY`.
fn nredis_incr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nredis_incr", span)?;
    let id = handle_arg(args, 0, "nredis_incr", span)?;
    let key = string_arg(args, 1, "nredis_incr", span)?;
    let by = if args.len() == 3 {
        int_arg(args, 2, "nredis_incr", span)?
    } else {
        1
    };
    handles::with_client_mut(id, "nredis_incr", span, |c| {
        c.incr(&key, by).map_err(|e| e.to_string())
    })
    .map(ok_int)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_expire(id, key, secs) -> bool`
fn nredis_expire(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nredis_expire", span)?;
    let id = handle_arg(args, 0, "nredis_expire", span)?;
    let key = string_arg(args, 1, "nredis_expire", span)?;
    let secs = int_arg(args, 2, "nredis_expire", span)?;
    let secs = secs.max(0) as u64;
    handles::with_client_mut(id, "nredis_expire", span, |c| {
        c.expire(&key, secs).map_err(|e| e.to_string())
    })
    .map(|b| Value::Bool(b).ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_mget(id, keys[]) -> array`
///
/// Returns an array of strings/nils in the same order as `keys`.
fn nredis_mget(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nredis_mget", span)?;
    let id = handle_arg(args, 0, "nredis_mget", span)?;
    let keys = string_array_arg(args, 1, "nredis_mget", span)?;
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    handles::with_client_mut(id, "nredis_mget", span, |c| {
        c.mget(&key_refs).map_err(|e| e.to_string())
    })
    .map(|vals| {
        let arr: Vec<ValueRef> = vals
            .into_iter()
            .map(|opt| match opt {
                Some(s) => Value::String(s).ref_cell(),
                None => Value::Nil.ref_cell(),
            })
            .collect();
        Value::Array(arr).ref_cell()
    })
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_mset(id, pairs{}) -> true`
///
/// `pairs` is an object with string (or stringifiable) values.
fn nredis_mset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nredis_mset", span)?;
    let id = handle_arg(args, 0, "nredis_mset", span)?;
    let pairs = object_pairs_arg(args, 1, "nredis_mset", span)?;
    // Collect owned strings before borrowing as &str slices.
    let owned: Vec<(String, String)> = pairs.into_iter().collect();
    let pair_refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    handles::with_client_mut(id, "nredis_mset", span, |c| {
        c.mset(&pair_refs).map_err(|e| e.to_string())
    })
    .map(|_| ok_true())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_hget(id, key, field) -> string | nil`
fn nredis_hget(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nredis_hget", span)?;
    let id = handle_arg(args, 0, "nredis_hget", span)?;
    let key = string_arg(args, 1, "nredis_hget", span)?;
    let field = string_arg(args, 2, "nredis_hget", span)?;
    handles::with_client_mut(id, "nredis_hget", span, |c| {
        c.hget(&key, &field).map_err(|e| e.to_string())
    })
    .map(|opt| match opt {
        Some(s) => Value::String(s).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_hset(id, key, field, value) -> true`
fn nredis_hset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nredis_hset", span)?;
    let id = handle_arg(args, 0, "nredis_hset", span)?;
    let key = string_arg(args, 1, "nredis_hset", span)?;
    let field = string_arg(args, 2, "nredis_hset", span)?;
    let val = string_arg(args, 3, "nredis_hset", span)?;
    handles::with_client_mut(id, "nredis_hset", span, |c| {
        c.hset(&key, &field, &val).map_err(|e| e.to_string())
    })
    .map(|_| ok_true())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_hdel(id, key, field) -> true`
fn nredis_hdel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nredis_hdel", span)?;
    let id = handle_arg(args, 0, "nredis_hdel", span)?;
    let key = string_arg(args, 1, "nredis_hdel", span)?;
    let field = string_arg(args, 2, "nredis_hdel", span)?;
    handles::with_client_mut(id, "nredis_hdel", span, |c| {
        c.hdel(&key, &field).map_err(|e| e.to_string())
    })
    .map(|_| ok_true())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_hgetall(id, key) -> object`
///
/// Returns an object with all field → value pairs of the hash.
fn nredis_hgetall(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nredis_hgetall", span)?;
    let id = handle_arg(args, 0, "nredis_hgetall", span)?;
    let key = string_arg(args, 1, "nredis_hgetall", span)?;
    handles::with_client_mut(id, "nredis_hgetall", span, |c| {
        c.hgetall(&key).map_err(|e| e.to_string())
    })
    .map(|pairs| {
        let mut map = HashMap::with_capacity(pairs.len());
        for (k, v) in pairs {
            map.insert(k, Value::String(v).ref_cell());
        }
        Value::Object(map).ref_cell()
    })
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_cmd(id, parts[]) -> value`
///
/// Send a raw RESP command. `parts` is an array of strings (command + args).
/// The first element must be the Redis command name (e.g. `"SET"`).
fn nredis_cmd(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nredis_cmd", span)?;
    let id = handle_arg(args, 0, "nredis_cmd", span)?;
    let parts = string_array_arg(args, 1, "nredis_cmd", span)?;
    if parts.is_empty() {
        return Ok(nredis_error(
            span,
            "nredis_cmd() parts array must not be empty",
        ));
    }
    // Convert to bytes; collect owned vecs to have stable references.
    let byte_vecs: Vec<Vec<u8>> = parts.iter().map(|s| s.as_bytes().to_vec()).collect();
    let byte_refs: Vec<&[u8]> = byte_vecs.iter().map(|v| v.as_slice()).collect();
    handles::with_client_mut(id, "nredis_cmd", span, |c| {
        c.raw_cmd(&byte_refs).map_err(|e| e.to_string())
    })
    .map(|v| resp_to_niao(v).ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// `nredis_close(id) -> true`
fn nredis_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nredis_close", span)?;
    let id = handle_arg(args, 0, "nredis_close", span)?;
    match handles::remove(id) {
        Some(_) => Ok(ok_true()),
        None => Ok(nredis_error(
            span,
            format!("Redis handle {id} not found or already closed"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Module surface
// ---------------------------------------------------------------------------

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("nredis_connect", Rc::new(nredis_connect)),
        ("nredis_ping", Rc::new(nredis_ping)),
        ("nredis_get", Rc::new(nredis_get)),
        ("nredis_set", Rc::new(nredis_set)),
        ("nredis_del", Rc::new(nredis_del)),
        ("nredis_incr", Rc::new(nredis_incr)),
        ("nredis_expire", Rc::new(nredis_expire)),
        ("nredis_mget", Rc::new(nredis_mget)),
        ("nredis_mset", Rc::new(nredis_mset)),
        ("nredis_hget", Rc::new(nredis_hget)),
        ("nredis_hset", Rc::new(nredis_hset)),
        ("nredis_hdel", Rc::new(nredis_hdel)),
        ("nredis_hgetall", Rc::new(nredis_hgetall)),
        ("nredis_cmd", Rc::new(nredis_cmd)),
        ("nredis_close", Rc::new(nredis_close)),
    ]
}

/// Namespace object exposed as `nredis` in the Niao environment.
pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "connect", Rc::new(nredis_connect));
    bind(&mut map, "ping", Rc::new(nredis_ping));
    bind(&mut map, "get", Rc::new(nredis_get));
    bind(&mut map, "set", Rc::new(nredis_set));
    bind(&mut map, "del", Rc::new(nredis_del));
    bind(&mut map, "incr", Rc::new(nredis_incr));
    bind(&mut map, "expire", Rc::new(nredis_expire));
    bind(&mut map, "mget", Rc::new(nredis_mget));
    bind(&mut map, "mset", Rc::new(nredis_mset));
    bind(&mut map, "hget", Rc::new(nredis_hget));
    bind(&mut map, "hset", Rc::new(nredis_hset));
    bind(&mut map, "hdel", Rc::new(nredis_hdel));
    bind(&mut map, "hgetall", Rc::new(nredis_hgetall));
    bind(&mut map, "cmd", Rc::new(nredis_cmd));
    bind(&mut map, "close", Rc::new(nredis_close));
    Value::Object(map)
}

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

pub const MODULE_NAME: &str = "nredis";
pub const MODULE_PATHS: &[&str] = &["nredis", "std/nredis"];

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    // --- resp_to_niao conversion ---

    #[test]
    fn resp_null_to_niao_nil() {
        assert!(matches!(
            resp_to_niao(niao_db::resp::Value::Null),
            Value::Nil
        ));
        assert!(matches!(
            resp_to_niao(niao_db::resp::Value::BulkString(None)),
            Value::Nil
        ));
    }

    #[test]
    fn resp_bulk_string_to_niao_string() {
        let v = resp_to_niao(niao_db::resp::Value::BulkString(Some(b"hello".to_vec())));
        assert!(matches!(v, Value::String(ref s) if s == "hello"));
    }

    #[test]
    fn resp_simple_string_to_niao_string() {
        let v = resp_to_niao(niao_db::resp::Value::SimpleString("OK".into()));
        assert!(matches!(v, Value::String(ref s) if s == "OK"));
    }

    #[test]
    fn resp_integer_to_niao_int() {
        let v = resp_to_niao(niao_db::resp::Value::Integer(42));
        assert!(matches!(v, Value::Int(42)));
    }

    #[test]
    fn resp_array_to_niao_array() {
        let v = resp_to_niao(niao_db::resp::Value::Array(vec![
            niao_db::resp::Value::Integer(1),
            niao_db::resp::Value::BulkString(None),
        ]));
        match v {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(*items[0].borrow(), Value::Int(1)));
                assert!(matches!(*items[1].borrow(), Value::Nil));
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn resp_error_to_niao_string() {
        let v = resp_to_niao(niao_db::resp::Value::Error("ERR bad".into()));
        assert!(matches!(v, Value::String(ref s) if s == "ERR bad"));
    }

    // --- Arity / type errors (no live Redis needed) ---

    #[test]
    fn connect_arity_error() {
        let span = dummy_span();
        let result = nredis_connect(&[], span);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::E2780_NREDIS_ARITY);
    }

    #[test]
    fn get_arity_error() {
        let span = dummy_span();
        let result = nredis_get(&[], span);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), codes::E2780_NREDIS_ARITY);
    }

    #[test]
    fn ping_type_error_bad_handle() {
        let span = dummy_span();
        // Passing a string instead of an int handle → E2783
        let bad = Value::String("bad".into()).ref_cell();
        let result = nredis_ping(&[bad], span);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            codes::E2783_NREDIS_INVALID_HANDLE
        );
    }

    #[test]
    fn invalid_handle_returns_recoverable_error() {
        let span = dummy_span();
        // Handle 99999 doesn't exist — with_client_mut returns RuntimeError which
        // gets converted to Ok(error_value) via error_from_runtime.
        let h = Value::Int(99999).ref_cell();
        let result = nredis_ping(&[h], span);
        // Must be Ok (recoverable), not Err.
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(matches!(*val.borrow(), Value::Error(_)));
    }

    #[test]
    fn close_unknown_handle_recoverable() {
        let span = dummy_span();
        let h = Value::Int(88888).ref_cell();
        let result = nredis_close(&[h], span);
        assert!(result.is_ok());
        assert!(matches!(*result.unwrap().borrow(), Value::Error(_)));
    }

    #[test]
    fn mget_requires_array() {
        let span = dummy_span();
        let h = Value::Int(1).ref_cell();
        let not_array = Value::String("k".into()).ref_cell();
        let result = nredis_mget(&[h, not_array], span);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), codes::E2782_NREDIS_TYPE);
    }

    #[test]
    fn mset_requires_object() {
        let span = dummy_span();
        let h = Value::Int(1).ref_cell();
        let not_obj = Value::String("k".into()).ref_cell();
        let result = nredis_mset(&[h, not_obj], span);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), codes::E2782_NREDIS_TYPE);
    }

    #[test]
    fn builtin_count_matches_expected() {
        assert_eq!(all_builtins().len(), 15);
    }

    #[test]
    fn namespace_has_all_keys() {
        let ns = namespace();
        if let Value::Object(map) = ns {
            for key in &[
                "connect", "ping", "get", "set", "del", "incr", "expire", "mget", "mset", "hget",
                "hset", "hdel", "hgetall", "cmd", "close",
            ] {
                assert!(map.contains_key(*key), "missing key: {key}");
            }
        } else {
            panic!("namespace() must return Object");
        }
    }
}
