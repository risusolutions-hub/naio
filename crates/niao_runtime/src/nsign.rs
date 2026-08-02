//! Native `nsign` standard library — signed + expiring tokens, cookies, URLs
//! (tamper-proof values). ~itsdangerous subset.
//!
//! Import with `import "nsign"` (or `import "std/nsign"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_sign::{
    cookie::{format_set_cookie, sign_cookie_value, unsign_cookie_value},
    sign_url, unsign_url, default_param, Digest, KeyDerivation, Serializer, SerializerKind,
    SerializerOptions, SignError, Signer, SignerConfig, TimestampSigner,
};
use niao_json_core::Value as JsonValue;
use serde_json::Map;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3581_NSIGN_ARITY: u32 = codes::E3581_NSIGN_ARITY;
const E3582_NSIGN_ERROR: u32 = codes::E3582_NSIGN_ERROR;
const E3583_NSIGN_TYPE: u32 = codes::E3583_NSIGN_TYPE;
const E3584_NSIGN_INVALID_HANDLE: u32 = codes::E3584_NSIGN_INVALID_HANDLE;
const E3585_NSIGN_EXPIRED: u32 = codes::E3585_NSIGN_EXPIRED;

// ---------------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------------

enum NsignHandle {
    Signer(Signer),
    Timed(TimestampSigner),
    Serializer(Serializer),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, NsignHandle>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn register(handle: NsignHandle) -> i64 {
    let id = new_handle();
    HANDLES.with(|m| m.borrow_mut().insert(id, handle));
    id
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut NsignHandle) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(h) => Ok(Ok(f(h))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3583_NSIGN_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3581_NSIGN_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3581_NSIGN_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nsign_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3582_NSIGN_ERROR, "nsign_error", msg.into(), span)
}

fn nsign_expired(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3585_NSIGN_EXPIRED, "nsign_expired", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3584_NSIGN_INVALID_HANDLE,
        "nsign_error",
        format!("invalid or closed nsign handle {id}"),
        span,
    )
}

fn map_sign_err(span: Span, e: SignError) -> ValueRef {
    match e {
        SignError::Expired { age_secs, max_age } => nsign_expired(
            span,
            format!("signature expired (age {age_secs}s > max {max_age}s)"),
        ),
        other => nsign_err(span, other.to_string()),
    }
}

fn str_val(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

fn int_val(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn bool_val(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
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

fn bytes_arg(secret: &str) -> Vec<u8> {
    secret.into_bytes()
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!("expected options object, got {}", other.type_name()),
        )),
    }
}

fn obj_str(map: &HashMap<String, ValueRef>, key: &str, default: &str) -> String {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => s,
        _ => default.to_string(),
    }
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        _ => default,
    }
}

fn opts_to_serializer(opts: &HashMap<String, ValueRef>) -> SerializerOptions {
    let salt = opts.get("salt").and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.as_bytes().to_vec()),
        _ => None,
    });
    let sep = opts.get("sep").and_then(|v| match &*v.borrow() {
        Value::String(s) if !s.is_empty() => Some(s.as_bytes()[0]),
        _ => None,
    });
    let digest = opts
        .get("digest")
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Digest::from_name(s),
            _ => None,
        });
    let key_derivation = opts.get("key_derivation").and_then(|v| match &*v.borrow() {
        Value::String(s) => KeyDerivation::from_name(s),
        _ => None,
    });
    let max_age = opts.get("max_age").and_then(|v| match &*v.borrow() {
        Value::Int(n) if *n >= 0 => Some(*n as u64),
        _ => None,
    });
    let max_payload = opts.get("max_payload").and_then(|v| match &*v.borrow() {
        Value::Int(n) if *n > 0 => Some(*n as usize),
        _ => None,
    });
    SerializerOptions {
        salt,
        sep,
        digest,
        key_derivation,
        max_age,
        max_payload,
    }
}

fn secret_keys_from_opts(secret: &str, opts: &HashMap<String, ValueRef>) -> Vec<Vec<u8>> {
    if let Some(v) = opts.get("secret_keys") {
        if let Value::Array(items) = &*v.borrow() {
            let keys: Vec<Vec<u8>> = items
                .iter()
                .filter_map(|item| match &*item.borrow() {
                    Value::String(s) => Some(s.as_bytes().to_vec()),
                    _ => None,
                })
                .collect();
            if !keys.is_empty() {
                return keys;
            }
        }
    }
    vec![bytes_arg(secret)]
}

fn to_json(v: &Value, span: Span) -> NiaoResult<JsonValue> {
    match v {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(n) => Ok(JsonValue::Number((*n).into())),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .ok_or_else(|| type_err(span, format!("non-finite float {f}"))),
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(to_json(&*item.borrow(), span)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, vr) in map {
                out.insert(k.clone(), to_json(&*vr.borrow(), span)?);
            }
            Ok(JsonValue::Object(out))
        }
        Value::BigInt(n) => {
            let s = n.to_string();
            if let Ok(i) = s.parse::<i64>() {
                return Ok(JsonValue::Number(i.into()));
            }
            Err(type_err(span, format!("bigint {s} is not JSON-representable")))
        }
        other => Err(type_err(
            span,
            format!(
                "JSON values must be nil, bool, number, string, array, or object — got {}",
                other.type_name()
            ),
        )),
    }
}

fn from_json(v: JsonValue) -> ValueRef {
    match v {
        JsonValue::Null => Value::Nil.ref_cell(),
        JsonValue::Bool(b) => Value::Bool(b).ref_cell(),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i).ref_cell()
            } else if let Some(f) = n.as_f64() {
                Value::Float(f).ref_cell()
            } else {
                Value::String(n.to_string()).ref_cell()
            }
        }
        JsonValue::String(s) => Value::String(s).ref_cell(),
        JsonValue::Array(items) => {
            let out: Vec<ValueRef> = items.into_iter().map(from_json).collect();
            Value::Array(out).ref_cell()
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                out.insert(k, from_json(v));
            }
            Value::Object(out).ref_cell()
        }
    }
}

fn handle_object(id: i64, kind: &str, methods: HashMap<String, ValueRef>) -> ValueRef {
    let mut map = methods;
    map.insert("id".to_string(), int_val(id));
    map.insert("kind".to_string(), str_val(kind));
    Value::Object(map).ref_cell()
}

fn handle_id_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Object(map) => match map.get("id") {
            Some(v) => match &*v.borrow() {
                Value::Int(id) => Ok(*id),
                other => Err(type_err(
                    span,
                    format!("{name}() expects handle.id int, got {}", other.type_name()),
                )),
            },
            None => Err(type_err(span, format!("{name}() expects nsign handle object"))),
        },
        Value::Int(id) => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn make_signer(secret: &str, opts: &HashMap<String, ValueRef>) -> Result<Signer, SignError> {
    let ser_opts = opts_to_serializer(opts);
    let config = ser_opts.clone().into_config();
    let keys = secret_keys_from_opts(secret, opts);
    if keys.len() == 1 {
        Signer::new(&keys[0], config)
    } else {
        Signer::with_keys(keys, config)
    }
}

fn make_timed(secret: &str, opts: &HashMap<String, ValueRef>) -> Result<TimestampSigner, SignError> {
    let ser_opts = opts_to_serializer(opts);
    let config = ser_opts.into_config();
    let keys = secret_keys_from_opts(secret, opts);
    if keys.len() == 1 {
        TimestampSigner::new(&keys[0], config)
    } else {
        TimestampSigner::with_keys(keys, config)
    }
}

fn make_serializer(
    secret: &str,
    opts: &HashMap<String, ValueRef>,
    kind: SerializerKind,
    timed: bool,
) -> Result<Serializer, SignError> {
    let ser_opts = opts_to_serializer(opts);
    let mut config = ser_opts.clone().into_config();
    if config.salt == b"itsdangerous.Signer"[..] {
        config.salt = b"itsdangerous".to_vec();
    }
    let keys = secret_keys_from_opts(secret, opts);
    let mut ser = if keys.len() == 1 {
        if timed {
            Serializer::timed(&keys[0], config, kind)?
        } else {
            Serializer::new(&keys[0], config, kind)?
        }
    } else if timed {
        Serializer::with_keys(keys, config, kind, true)?
    } else {
        Serializer::with_keys(keys, config, kind, false)?
    };
    if let Some(max) = ser_opts.max_age {
        ser.set_default_max_age(Some(max));
    }
    Ok(ser)
}

// ---------------------------------------------------------------------------
// Handle factories
// ---------------------------------------------------------------------------

// >>> type(nsign.signer("secret").sign("hi"))
// "string"
fn nsign_signer(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsign_signer", span)?;
    let secret = string_arg(args, 0, "nsign_signer", span)?;
    let opts = parse_opts(args, 1, span)?;
    match make_signer(&secret, &opts) {
        Ok(signer) => {
            let id = register(NsignHandle::Signer(signer));
            let mut methods = HashMap::new();
            methods.insert(
                "sign".to_string(),
                Value::NativeFunction(Rc::new(nsign_signer_sign_method)).ref_cell(),
            );
            methods.insert(
                "unsign".to_string(),
                Value::NativeFunction(Rc::new(nsign_signer_unsign_method)).ref_cell(),
            );
            methods.insert(
                "validate".to_string(),
                Value::NativeFunction(Rc::new(nsign_signer_validate_method)).ref_cell(),
            );
            Ok(handle_object(id, "signer", methods))
        }
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

fn nsign_signer_sign_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "signer.sign", span)?;
    let id = handle_id_from_arg(args, 0, span, "signer.sign")?;
    let value = string_arg(args, 1, "signer.sign", span)?;
    match with_handle(id, span, |h| {
        if let NsignHandle::Signer(s) = h {
            s.sign(&value)
        } else {
            Err(SignError::BadFormat)
        }
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nsign_signer_unsign_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "signer.unsign", span)?;
    let id = handle_id_from_arg(args, 0, span, "signer.unsign")?;
    let signed = string_arg(args, 1, "signer.unsign", span)?;
    match with_handle(id, span, |h| {
        if let NsignHandle::Signer(s) = h {
            s.unsign(&signed)
        } else {
            Err(SignError::BadFormat)
        }
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nsign_signer_validate_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "signer.validate", span)?;
    let id = handle_id_from_arg(args, 0, span, "signer.validate")?;
    let signed = string_arg(args, 1, "signer.validate", span)?;
    match with_handle(id, span, |h| {
        if let NsignHandle::Signer(s) = h {
            Ok(s.validate(&signed))
        } else {
            Ok(false)
        }
    })? {
        Ok(Ok(b)) => bool_val(b),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> nsign.timed("secret", {max_age: 60}).sign("x") != ""
// true
fn nsign_timed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsign_timed", span)?;
    let secret = string_arg(args, 0, "nsign_timed", span)?;
    let opts = parse_opts(args, 1, span)?;
    match make_timed(&secret, &opts) {
        Ok(signer) => {
            let max_age = opts_to_serializer(&opts).max_age;
            let id = register(NsignHandle::Timed(signer));
            let mut methods = HashMap::new();
            methods.insert(
                "sign".to_string(),
                Value::NativeFunction(Rc::new(nsign_timed_sign_method)).ref_cell(),
            );
            methods.insert(
                "unsign".to_string(),
                Value::NativeFunction(Rc::new(nsign_timed_unsign_method)).ref_cell(),
            );
            methods.insert(
                "validate".to_string(),
                Value::NativeFunction(Rc::new(nsign_timed_validate_method)).ref_cell(),
            );
            if let Some(m) = max_age {
                methods.insert("max_age".to_string(), int_val(m as i64));
            }
            Ok(handle_object(id, "timed", methods))
        }
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

fn nsign_timed_sign_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "timed.sign", span)?;
    let id = handle_id_from_arg(args, 0, span, "timed.sign")?;
    let value = string_arg(args, 1, "timed.sign", span)?;
    match with_handle(id, span, |h| {
        if let NsignHandle::Timed(s) = h {
            s.sign(&value)
        } else {
            Err(SignError::BadFormat)
        }
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nsign_timed_unsign_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "timed.unsign", span)?;
    let id = handle_id_from_arg(args, 0, span, "timed.unsign")?;
    let signed = string_arg(args, 1, "timed.unsign", span)?;
    let max_age = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Int(n) if *n >= 0 => Some(*n as u64),
            Value::Nil => None,
            _ => None,
        }
    } else {
        None
    };
    match with_handle(id, span, |h| {
        if let NsignHandle::Timed(s) = h {
            s.unsign(&signed, max_age)
        } else {
            Err(SignError::BadFormat)
        }
    })? {
        Ok(Ok((val, ts))) => {
            let mut map = HashMap::new();
            map.insert("value".to_string(), str_val(val));
            map.insert("timestamp".to_string(), int_val(ts as i64));
            Ok(Value::Object(map).ref_cell())
        }
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nsign_timed_validate_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "timed.validate", span)?;
    let id = handle_id_from_arg(args, 0, span, "timed.validate")?;
    let signed = string_arg(args, 1, "timed.validate", span)?;
    let max_age = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Int(n) if *n >= 0 => Some(*n as u64),
            _ => None,
        }
    } else {
        None
    };
    match with_handle(id, span, |h| {
        if let NsignHandle::Timed(s) = h {
            Ok(s.validate(&signed, max_age))
        } else {
            Ok(false)
        }
    })? {
        Ok(Ok(b)) => bool_val(b),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn serializer_handle(
    secret: &str,
    opts: &HashMap<String, ValueRef>,
    kind: SerializerKind,
    timed: bool,
    kind_name: &str,
    span: Span,
) -> NiaoResult<ValueRef> {
    match make_serializer(secret, opts, kind, timed) {
        Ok(ser) => {
            let max_age = opts_to_serializer(opts).max_age;
            let id = register(NsignHandle::Serializer(ser));
            let mut methods = HashMap::new();
            methods.insert(
                "dumps".to_string(),
                Value::NativeFunction(Rc::new(nsign_serializer_dumps_method)).ref_cell(),
            );
            methods.insert(
                "loads".to_string(),
                Value::NativeFunction(Rc::new(nsign_serializer_loads_method)).ref_cell(),
            );
            methods.insert(
                "loads_unsafe".to_string(),
                Value::NativeFunction(Rc::new(nsign_serializer_loads_unsafe_method)).ref_cell(),
            );
            methods.insert(
                "validate".to_string(),
                Value::NativeFunction(Rc::new(nsign_serializer_validate_method)).ref_cell(),
            );
            if let Some(m) = max_age {
                methods.insert("max_age".to_string(), int_val(m as i64));
            }
            Ok(handle_object(id, kind_name, methods))
        }
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> type(nsign.serializer("k").dumps({a: 1}))
// "string"
fn nsign_serializer(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsign_serializer", span)?;
    let secret = string_arg(args, 0, "nsign_serializer", span)?;
    let opts = parse_opts(args, 1, span)?;
    serializer_handle(&secret, &opts, SerializerKind::Json, false, "serializer", span)
}

// >>> type(nsign.url_safe("k").dumps({a: 1}))
// "string"
fn nsign_url_safe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsign_url_safe", span)?;
    let secret = string_arg(args, 0, "nsign_url_safe", span)?;
    let opts = parse_opts(args, 1, span)?;
    serializer_handle(&secret, &opts, SerializerKind::UrlSafe, true, "url_safe", span)
}

fn nsign_serializer_dumps_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "serializer.dumps", span)?;
    let id = handle_id_from_arg(args, 0, span, "serializer.dumps")?;
    let json = to_json(&*args[1].borrow(), span)?;
    match with_handle(id, span, |h| {
        if let NsignHandle::Serializer(s) = h {
            s.dumps_json(&json)
        } else {
            Err(SignError::BadFormat)
        }
    })? {
        Ok(Ok(tok)) => Ok(str_val(tok)),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nsign_serializer_loads_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "serializer.loads", span)?;
    let id = handle_id_from_arg(args, 0, span, "serializer.loads")?;
    let token = string_arg(args, 1, "serializer.loads", span)?;
    let max_age = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Int(n) if *n >= 0 => Some(*n as u64),
            Value::Nil => None,
            _ => None,
        }
    } else {
        None
    };
    match with_handle(id, span, |h| {
        if let NsignHandle::Serializer(s) = h {
            s.loads_json(&token, max_age)
        } else {
            Err(SignError::BadFormat)
        }
    })? {
        Ok(Ok(v)) => Ok(from_json(v)),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nsign_serializer_loads_unsafe_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "serializer.loads_unsafe", span)?;
    let id = handle_id_from_arg(args, 0, span, "serializer.loads_unsafe")?;
    let token = string_arg(args, 1, "serializer.loads_unsafe", span)?;
    let max_age = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Int(n) if *n >= 0 => Some(*n as u64),
            _ => None,
        }
    } else {
        None
    };
    match with_handle(id, span, |h| {
        if let NsignHandle::Serializer(s) = h {
            Ok(s.loads_unsafe_json(&token, max_age))
        } else {
            Err(SignError::BadFormat)
        }
    })? {
        Ok(Ok(r)) => {
            let mut map = HashMap::new();
            map.insert("valid".to_string(), bool_val(r.valid));
            map.insert(
                "value".to_string(),
                r.value.map(from_json).unwrap_or(Value::Nil.ref_cell()),
            );
            map.insert(
                "timestamp".to_string(),
                r.timestamp
                    .map(|t| int_val(t as i64))
                    .unwrap_or(Value::Nil.ref_cell()),
            );
            map.insert("expired".to_string(), bool_val(r.expired));
            if let Some(e) = r.error {
                map.insert("error".to_string(), str_val(e));
            }
            Ok(Value::Object(map).ref_cell())
        }
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nsign_serializer_validate_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "serializer.validate", span)?;
    let id = handle_id_from_arg(args, 0, span, "serializer.validate")?;
    let token = string_arg(args, 1, "serializer.validate", span)?;
    let max_age = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Int(n) if *n >= 0 => Some(*n as u64),
            _ => None,
        }
    } else {
        None
    };
    match with_handle(id, span, |h| {
        if let NsignHandle::Serializer(s) = h {
            Ok(s.loads_json(&token, max_age).is_ok())
        } else {
            Ok(false)
        }
    })? {
        Ok(Ok(b)) => bool_val(b),
        Ok(Err(e)) => Ok(map_sign_err(span, e)),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// One-shot helpers
// ---------------------------------------------------------------------------

// >>> nsign.sign("hello", "secret") != ""
// true
fn nsign_sign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_sign", span)?;
    let value = string_arg(args, 0, "nsign_sign", span)?;
    let secret = string_arg(args, 1, "nsign_sign", span)?;
    let opts = parse_opts(args, 2, span)?;
    match make_signer(&secret, &opts) {
        Ok(s) => match s.sign(&value) {
            Ok(tok) => Ok(str_val(tok)),
            Err(e) => Ok(map_sign_err(span, e)),
        },
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> nsign.unsign(nsign.sign("x", "secret"), "secret")
// "x"
fn nsign_unsign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_unsign", span)?;
    let signed = string_arg(args, 0, "nsign_unsign", span)?;
    let secret = string_arg(args, 1, "nsign_unsign", span)?;
    let opts = parse_opts(args, 2, span)?;
    match make_signer(&secret, &opts) {
        Ok(s) => match s.unsign(&signed) {
            Ok(v) => Ok(str_val(v)),
            Err(e) => Ok(map_sign_err(span, e)),
        },
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> type(nsign.dumps({n: 1}, "secret"))
// "string"
fn nsign_dumps(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_dumps", span)?;
    let json = to_json(&*args[0].borrow(), span)?;
    let secret = string_arg(args, 1, "nsign_dumps", span)?;
    let opts = parse_opts(args, 2, span)?;
    let url_safe = obj_bool(&opts, "url_safe", true);
    let kind = if url_safe {
        SerializerKind::UrlSafe
    } else {
        SerializerKind::Json
    };
    match make_serializer(&secret, &opts, kind, true) {
        Ok(ser) => match ser.dumps_json(&json) {
            Ok(tok) => Ok(str_val(tok)),
            Err(e) => Ok(map_sign_err(span, e)),
        },
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> nsign.loads(nsign.dumps({n: 1}, "secret"), "secret").n
// 1
fn nsign_loads(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_loads", span)?;
    let token = string_arg(args, 0, "nsign_loads", span)?;
    let secret = string_arg(args, 1, "nsign_loads", span)?;
    let opts = parse_opts(args, 2, span)?;
    let url_safe = obj_bool(&opts, "url_safe", true);
    let kind = if url_safe {
        SerializerKind::UrlSafe
    } else {
        SerializerKind::Json
    };
    match make_serializer(&secret, &opts, kind, true) {
        Ok(ser) => match ser.loads_json(&token, ser_opts_max_age(&opts)) {
            Ok(v) => Ok(from_json(v)),
            Err(e) => Ok(map_sign_err(span, e)),
        },
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

fn ser_opts_max_age(opts: &HashMap<String, ValueRef>) -> Option<u64> {
    opts_to_serializer(opts).max_age
}

// >>> nsign.loads_unsafe("bad", "secret").valid
// false
fn nsign_loads_unsafe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_loads_unsafe", span)?;
    let token = string_arg(args, 0, "nsign_loads_unsafe", span)?;
    let secret = string_arg(args, 1, "nsign_loads_unsafe", span)?;
    let opts = parse_opts(args, 2, span)?;
    let url_safe = obj_bool(&opts, "url_safe", true);
    let kind = if url_safe {
        SerializerKind::UrlSafe
    } else {
        SerializerKind::Json
    };
    match make_serializer(&secret, &opts, kind, true) {
        Ok(ser) => {
            let r = ser.loads_unsafe_json(&token, ser_opts_max_age(&opts));
            let mut map = HashMap::new();
            map.insert("valid".to_string(), bool_val(r.valid));
            map.insert(
                "value".to_string(),
                r.value.map(from_json).unwrap_or(Value::Nil.ref_cell()),
            );
            map.insert(
                "timestamp".to_string(),
                r.timestamp
                    .map(|t| int_val(t as i64))
                    .unwrap_or(Value::Nil.ref_cell()),
            );
            map.insert("expired".to_string(), bool_val(r.expired));
            if let Some(e) = r.error {
                map.insert("error".to_string(), str_val(e));
            }
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> nsign.validate(nsign.dumps({x:1}, "s"), "s")
// true
fn nsign_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_validate", span)?;
    let token = string_arg(args, 0, "nsign_validate", span)?;
    let secret = string_arg(args, 1, "nsign_validate", span)?;
    let opts = parse_opts(args, 2, span)?;
    let url_safe = obj_bool(&opts, "url_safe", true);
    let kind = if url_safe {
        SerializerKind::UrlSafe
    } else {
        SerializerKind::Json
    };
    match make_serializer(&secret, &opts, kind, true) {
        Ok(ser) => bool_val(ser.loads_json(&token, ser_opts_max_age(&opts)).is_ok()),
        Err(_) => bool_val(false),
    }
}

// >>> type(nsign.cookie_sign("sid", {u: 1}, "secret"))
// "string"
fn nsign_cookie_sign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nsign_cookie_sign", span)?;
    let name = string_arg(args, 0, "nsign_cookie_sign", span)?;
    let json = to_json(&*args[1].borrow(), span)?;
    let secret = string_arg(args, 2, "nsign_cookie_sign", span)?;
    let opts = parse_opts(args, 3, span)?;
    let ser_opts = opts_to_serializer(&opts);
    match sign_cookie_value(&name, &json, secret.as_bytes(), &ser_opts) {
        Ok(v) => Ok(str_val(v)),
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> nsign.cookie_unsign("sid=" + nsign.cookie_sign("sid", {u:1}, "secret"), "secret").u
// 1
fn nsign_cookie_unsign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_cookie_unsign", span)?;
    let cookie = string_arg(args, 0, "nsign_cookie_unsign", span)?;
    let secret = string_arg(args, 1, "nsign_cookie_unsign", span)?;
    let opts = parse_opts(args, 2, span)?;
    let ser_opts = opts_to_serializer(&opts);
    match unsign_cookie_value(&cookie, secret.as_bytes(), &ser_opts) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> nsign.set_cookie("sid", "val", {max_age: 60}).contains("HttpOnly")
// true
fn nsign_set_cookie(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_set_cookie", span)?;
    let name = string_arg(args, 0, "nsign_set_cookie", span)?;
    let value = string_arg(args, 1, "nsign_set_cookie", span)?;
    let opts = parse_opts(args, 2, span)?;
    let max_age = opts.get("max_age").and_then(|v| match &*v.borrow() {
        Value::Int(n) if *n >= 0 => Some(*n as u64),
        _ => None,
    });
    let path = obj_str(&opts, "path", "/");
    let http_only = obj_bool(&opts, "http_only", true);
    let secure = obj_bool(&opts, "secure", false);
    let same_site = opts.get("same_site").and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.as_str()),
        _ => None,
    });
    Ok(str_val(format_set_cookie(
        &name,
        &value,
        max_age,
        &path,
        http_only,
        secure,
        same_site,
    )))
}

// >>> nsign.sign_url("https://x.com", {id: 1}, "secret").contains("token=")
// true
fn nsign_sign_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nsign_sign_url", span)?;
    let base = string_arg(args, 0, "nsign_sign_url", span)?;
    let json = to_json(&*args[1].borrow(), span)?;
    let secret = string_arg(args, 2, "nsign_sign_url", span)?;
    let opts = parse_opts(args, 3, span)?;
    let param = obj_str(&opts, "param", default_param());
    let ser_opts = opts_to_serializer(&opts);
    match sign_url(&base, &json, secret.as_bytes(), &ser_opts, &param) {
        Ok(u) => Ok(str_val(u)),
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// >>> nsign.unsign_url(nsign.sign_url("https://x.com", {id: 5}, "secret"), "secret").id
// 5
fn nsign_unsign_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsign_unsign_url", span)?;
    let url = string_arg(args, 0, "nsign_unsign_url", span)?;
    let secret = string_arg(args, 1, "nsign_unsign_url", span)?;
    let opts = parse_opts(args, 2, span)?;
    let param = obj_str(&opts, "param", default_param());
    let ser_opts = opts_to_serializer(&opts);
    match unsign_url(&url, secret.as_bytes(), &ser_opts, &param) {
        Ok(v) => Ok(from_json(v)),
        Err(e) => Ok(map_sign_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsign_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsign_fns![
    ("nsign_signer", "signer", nsign_signer),
    ("nsign_timed", "timed", nsign_timed),
    ("nsign_serializer", "serializer", nsign_serializer),
    ("nsign_url_safe", "url_safe", nsign_url_safe),
    ("nsign_sign", "sign", nsign_sign),
    ("nsign_unsign", "unsign", nsign_unsign),
    ("nsign_dumps", "dumps", nsign_dumps),
    ("nsign_loads", "loads", nsign_loads),
    ("nsign_loads_unsafe", "loads_unsafe", nsign_loads_unsafe),
    ("nsign_validate", "validate", nsign_validate),
    ("nsign_cookie_sign", "cookie_sign", nsign_cookie_sign),
    ("nsign_cookie_unsign", "cookie_unsign", nsign_cookie_unsign),
    ("nsign_set_cookie", "set_cookie", nsign_set_cookie),
    ("nsign_sign_url", "sign_url", nsign_sign_url),
    ("nsign_unsign_url", "unsign_url", nsign_unsign_url),
];

pub const MODULE_NAME: &str = "nsign";
pub const MODULE_PATHS: &[&str] = &["nsign", "std/nsign"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert("DEFAULT_PARAM".to_string(), str_val(default_param()));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn sign_unsign_roundtrip() {
        let signed = nsign_sign(
            &[str_val("payload"), str_val("test-secret")],
            span(),
        )
        .unwrap();
        let out = nsign_unsign(&[signed, str_val("test-secret")], span()).unwrap();
        match &*out.borrow() {
            Value::String(s) => assert_eq!(s, "payload"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn dumps_loads_roundtrip() {
        let mut fields = HashMap::new();
        fields.insert("n".to_string(), Value::Int(42).ref_cell());
        let obj = Value::Object(fields).ref_cell();
        let tok = nsign_dumps(&[obj.clone(), str_val("secret")], span()).unwrap();
        let out = nsign_loads(&[tok, str_val("secret")], span()).unwrap();
        match &*out.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("n").unwrap().borrow(), Value::Int(42)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
