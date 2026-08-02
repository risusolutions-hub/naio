//! Native `notp` standard library — TOTP/HOTP two-factor codes, provisioning URIs
//! (~pyotp subset).
//!
//! Import with `import "notp"` (or `import "std/notp"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_otp::{
    base32_decode, base32_encode, hotp_at, hotp_at_bulk, parse_uri, random_base32, totp_at,
    totp_at_bulk, verify_token, Digest, Hotp, OtpError, ParsedOtp, Totp, DEFAULT_DIGITS,
    DEFAULT_INTERVAL, MAX_DIGITS, MIN_DIGITS,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

const E3572_NOTP_ARITY: u32 = codes::E3572_NOTP_ARITY;
const E3573_NOTP_ERROR: u32 = codes::E3573_NOTP_ERROR;
const E3574_NOTP_TYPE: u32 = codes::E3574_NOTP_TYPE;
const E3575_NOTP_INVALID_HANDLE: u32 = codes::E3575_NOTP_INVALID_HANDLE;

enum OtpHandle {
    Totp(Totp),
    Hotp(Hotp),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, OtpHandle>> = RefCell::new(HashMap::new());
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

fn register(handle: OtpHandle) -> i64 {
    let id = new_handle();
    HANDLES.with(|m| m.borrow_mut().insert(id, handle));
    id
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut OtpHandle) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(h) => Ok(Ok(f(h))),
            None => Ok(Err(error_value(
                E3575_NOTP_INVALID_HANDLE,
                "notp_error",
                format!("invalid or closed notp handle {id}"),
                span,
            ))),
        }
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3574_NOTP_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3572_NOTP_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3572_NOTP_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn notp_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3573_NOTP_ERROR, "notp_error", msg.into(), span)
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

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        Value::Nil => default,
        _ => default,
    }
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
            format!("opts must be an object, got {}", other.type_name()),
        )),
    }
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_string_opt(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn parse_digest(map: &HashMap<String, ValueRef>, span: Span) -> Result<Digest, ValueRef> {
    match obj_string_opt(map, "digest") {
        Some(s) => Digest::parse(&s).map_err(|e| notp_err(span, e.to_string())),
        None => Ok(Digest::Sha1),
    }
}

fn parse_digits(map: &HashMap<String, ValueRef>, span: Span) -> Result<u32, ValueRef> {
    let d = obj_int(map, "digits", DEFAULT_DIGITS as i64) as u32;
    if !(MIN_DIGITS..=MAX_DIGITS).contains(&d) {
        return Err(notp_err(
            span,
            format!("digits must be {MIN_DIGITS}..={MAX_DIGITS}, got {d}"),
        ));
    }
    Ok(d)
}

fn parse_interval(map: &HashMap<String, ValueRef>, span: Span) -> Result<u64, ValueRef> {
    let i = obj_int(map, "interval", DEFAULT_INTERVAL as i64) as u64;
    if i == 0 {
        return Err(notp_err(span, "interval must be > 0"));
    }
    Ok(i)
}

fn system_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn handle_id_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Object(map) => match map.get("id") {
            Some(v) => match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(type_err(
                    span,
                    format!("{name}() handle.id must be int, got {}", other.type_name()),
                )),
            },
            None => Err(type_err(span, format!("{name}() missing handle id"))),
        },
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a handle object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_object(id: i64, kind: &str, mut fields: HashMap<String, ValueRef>) -> ValueRef {
    fields.insert("id".to_string(), int_val(id));
    fields.insert("kind".to_string(), str_val(kind));
    Value::Object(fields).ref_cell()
}

fn map_otp_err(e: OtpError) -> String {
    e.to_string()
}

fn totp_handle_object(t: Totp) -> ValueRef {
    let id = register(OtpHandle::Totp(t.clone()));
    let mut methods = HashMap::new();
    methods.insert(
        "now".to_string(),
        Value::NativeFunction(Rc::new(notp_totp_now_method)).ref_cell(),
    );
    methods.insert(
        "at".to_string(),
        Value::NativeFunction(Rc::new(notp_totp_at_method)).ref_cell(),
    );
    methods.insert(
        "verify".to_string(),
        Value::NativeFunction(Rc::new(notp_totp_verify_method)).ref_cell(),
    );
    methods.insert(
        "provisioning_uri".to_string(),
        Value::NativeFunction(Rc::new(notp_totp_provisioning_uri_method)).ref_cell(),
    );
    let mut fields = methods;
    fields.insert("digits".to_string(), int_val(t.digits() as i64));
    fields.insert("interval".to_string(), int_val(t.interval() as i64));
    fields.insert("digest".to_string(), str_val(t.digest().name()));
    fields.insert("secret".to_string(), str_val(t.secret_base32()));
    handle_object(id, "totp", fields)
}

fn hotp_handle_object(h: Hotp) -> ValueRef {
    let id = register(OtpHandle::Hotp(h.clone()));
    let mut methods = HashMap::new();
    methods.insert(
        "at".to_string(),
        Value::NativeFunction(Rc::new(notp_hotp_at_method)).ref_cell(),
    );
    methods.insert(
        "verify".to_string(),
        Value::NativeFunction(Rc::new(notp_hotp_verify_method)).ref_cell(),
    );
    methods.insert(
        "verify_window".to_string(),
        Value::NativeFunction(Rc::new(notp_hotp_verify_window_method)).ref_cell(),
    );
    methods.insert(
        "provisioning_uri".to_string(),
        Value::NativeFunction(Rc::new(notp_hotp_provisioning_uri_method)).ref_cell(),
    );
    let mut fields = methods;
    fields.insert("digits".to_string(), int_val(h.digits() as i64));
    fields.insert("digest".to_string(), str_val(h.digest().name()));
    fields.insert("secret".to_string(), str_val(h.secret_base32()));
    handle_object(id, "hotp", fields)
}

fn parsed_to_handle(parsed: ParsedOtp) -> ValueRef {
    match parsed {
        ParsedOtp::Totp(t) => totp_handle_object(t),
        ParsedOtp::Hotp(h) => hotp_handle_object(h),
    }
}

fn int_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects int array elements, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::IntArray(v) => Ok(v.clone()),
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

// ---------------------------------------------------------------------------
// Module functions
// ---------------------------------------------------------------------------

// >>> len(notp.random_base32())
// 32
fn notp_random_base32(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "notp_random_base32", span)?;
    let length = optional_int(args, 0, 32) as usize;
    match random_base32(length) {
        Ok(s) => Ok(str_val(s)),
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// >>> notp.compare("123456", "123456")
// true
fn notp_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "notp_compare", span)?;
    let a = string_arg(args, 0, "notp_compare", span)?;
    let b = string_arg(args, 1, "notp_compare", span)?;
    Ok(bool_val(verify_token(&a, &b)))
}

// >>> notp.base32_decode("JBSWY3DPEHPK3PXP")[0] >= 0
// true
fn notp_base32_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "notp_base32_decode", span)?;
    let s = string_arg(args, 0, "notp_base32_decode", span)?;
    match base32_decode(&s) {
        Ok(bytes) => {
            let arr: Vec<ValueRef> = bytes
                .iter()
                .map(|&b| Value::Int(b as i64).ref_cell())
                .collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// >>> notp.base32_encode(notp.base32_decode("JBSWY3DPEHPK3PXP"))
// "JBSWY3DPEHPK3PXP"
fn notp_base32_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "notp_base32_encode", span)?;
    match &*args[0].borrow() {
        Value::Array(items) => {
            let mut bytes = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Int(n) if (0..=255).contains(n) => bytes.push(*n as u8),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "base32_encode() bytes must be 0..=255, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(str_val(base32_encode(&bytes)))
        }
        other => Err(type_err(
            span,
            format!(
                "base32_encode() expects byte array, got {}",
                other.type_name()
            ),
        )),
    }
}

// >>> type(notp.totp("JBSWY3DPEHPK3PXP"))
// "object"
fn notp_totp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "notp_totp", span)?;
    let secret = string_arg(args, 0, "notp_totp", span)?;
    let opts = parse_opts(args, 1, span)?;
    let digits = match parse_digits(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let interval = match parse_interval(&opts, span) {
        Ok(i) => i,
        Err(e) => return Ok(e),
    };
    let digest = match parse_digest(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    match Totp::new(&secret, digits, interval, digest) {
        Ok(mut t) => {
            t = t.with_labels(
                obj_string_opt(&opts, "name"),
                obj_string_opt(&opts, "issuer"),
            );
            Ok(totp_handle_object(t))
        }
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// >>> type(notp.hotp("JBSWY3DPEHPK3PXP"))
// "object"
fn notp_hotp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "notp_hotp", span)?;
    let secret = string_arg(args, 0, "notp_hotp", span)?;
    let opts = parse_opts(args, 1, span)?;
    let digits = match parse_digits(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let digest = match parse_digest(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    match Hotp::new(&secret, digits, digest) {
        Ok(mut h) => {
            h = h.with_labels(
                obj_string_opt(&opts, "name"),
                obj_string_opt(&opts, "issuer"),
            );
            Ok(hotp_handle_object(h))
        }
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// >>> notp.totp_at("JBSWY3DPEHPK3PXP", 59)
// "287082"
fn notp_totp_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "notp_totp_at", span)?;
    let secret = string_arg(args, 0, "notp_totp_at", span)?;
    let time = int_arg(args, 1, "notp_totp_at", span)?;
    if time < 0 {
        return Ok(notp_err(span, "unix time must be >= 0"));
    }
    let opts = parse_opts(args, 2, span)?;
    let digits = match parse_digits(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let interval = match parse_interval(&opts, span) {
        Ok(i) => i,
        Err(e) => return Ok(e),
    };
    let digest = match parse_digest(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    match totp_at(&secret, time as u64, digits, interval, digest) {
        Ok(code) => Ok(str_val(code)),
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// >>> notp.hotp_at("JBSWY3DPEHPK3PXP", 0)
// "755224"
fn notp_hotp_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "notp_hotp_at", span)?;
    let secret = string_arg(args, 0, "notp_hotp_at", span)?;
    let counter = int_arg(args, 1, "notp_hotp_at", span)?;
    if counter < 0 {
        return Ok(notp_err(span, "counter must be >= 0"));
    }
    let opts = parse_opts(args, 2, span)?;
    let digits = match parse_digits(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let digest = match parse_digest(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    match hotp_at(&secret, counter as u64, digits, digest) {
        Ok(code) => Ok(str_val(code)),
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// >>> len(notp.totp_now("JBSWY3DPEHPK3PXP"))
// 6
fn notp_totp_now(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "notp_totp_now", span)?;
    let secret = string_arg(args, 0, "notp_totp_now", span)?;
    let opts_arg = args.get(1).cloned().unwrap_or(Value::Nil.ref_cell());
    notp_totp_at(
        &[
            Value::String(secret).ref_cell(),
            Value::Int(system_unix_s() as i64).ref_cell(),
            opts_arg,
        ],
        span,
    )
}

// >>> notp.parse_uri("otpauth://totp/Example:user?secret=JBSWY3DPEHPK3PXP&issuer=Example").ok
// true
fn notp_parse_uri(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "notp_parse_uri", span)?;
    let uri = string_arg(args, 0, "notp_parse_uri", span)?;
    match parse_uri(&uri) {
        Ok(parsed) => {
            let handle = parsed_to_handle(parsed);
            let mut map = HashMap::new();
            map.insert("ok".to_string(), bool_val(true));
            map.insert("value".to_string(), handle);
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => {
            let mut map = HashMap::new();
            map.insert("ok".to_string(), bool_val(false));
            map.insert("error".to_string(), str_val(map_otp_err(e)));
            Ok(Value::Object(map).ref_cell())
        }
    }
}

// >>> len(notp.totp_at_bulk("JBSWY3DPEHPK3PXP", [59, 1111111111]))
// 2
fn notp_totp_at_bulk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "notp_totp_at_bulk", span)?;
    let secret = string_arg(args, 0, "notp_totp_at_bulk", span)?;
    let times = int_array_arg(args, 1, "notp_totp_at_bulk", span)?;
    let opts = parse_opts(args, 2, span)?;
    let digits = match parse_digits(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let interval = match parse_interval(&opts, span) {
        Ok(i) => i,
        Err(e) => return Ok(e),
    };
    let digest = match parse_digest(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let mut times_u = Vec::with_capacity(times.len());
    for t in times {
        if t < 0 {
            return Ok(notp_err(span, "timestamps must be >= 0"));
        }
        times_u.push(t as u64);
    }
    match totp_at_bulk(&secret, &times_u, digits, interval, digest) {
        Ok(codes) => Ok(Value::Array(codes.into_iter().map(str_val).collect()).ref_cell()),
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// >>> len(notp.hotp_at_bulk("JBSWY3DPEHPK3PXP", [0, 1, 2]))
// 3
fn notp_hotp_at_bulk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "notp_hotp_at_bulk", span)?;
    let secret = string_arg(args, 0, "notp_hotp_at_bulk", span)?;
    let counters = int_array_arg(args, 1, "notp_hotp_at_bulk", span)?;
    let opts = parse_opts(args, 2, span)?;
    let digits = match parse_digits(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let digest = match parse_digest(&opts, span) {
        Ok(d) => d,
        Err(e) => return Ok(e),
    };
    let mut counters_u = Vec::with_capacity(counters.len());
    for c in counters {
        if c < 0 {
            return Ok(notp_err(span, "counters must be >= 0"));
        }
        counters_u.push(c as u64);
    }
    match hotp_at_bulk(&secret, &counters_u, digits, digest) {
        Ok(codes) => Ok(Value::Array(codes.into_iter().map(str_val).collect()).ref_cell()),
        Err(e) => Ok(notp_err(span, map_otp_err(e))),
    }
}

// ---------------------------------------------------------------------------
// Handle methods
// ---------------------------------------------------------------------------

fn notp_totp_now_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "totp.now", span)?;
    let id = handle_id_from_arg(args, 0, span, "totp.now")?;
    let now = system_unix_s();
    match with_handle(id, span, |h| {
        if let OtpHandle::Totp(t) = h {
            t.at(now)
        } else {
            String::new()
        }
    })? {
        Ok(s) if !s.is_empty() => Ok(str_val(s)),
        Ok(_) => Ok(notp_err(span, "invalid totp handle")),
        Err(e) => Ok(e),
    }
}

fn notp_totp_at_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "totp.at", span)?;
    let id = handle_id_from_arg(args, 0, span, "totp.at")?;
    let time = int_arg(args, 1, "totp.at", span)?;
    if time < 0 {
        return Ok(notp_err(span, "unix time must be >= 0"));
    }
    match with_handle(id, span, |h| {
        if let OtpHandle::Totp(t) = h {
            t.at(time as u64)
        } else {
            String::new()
        }
    })? {
        Ok(s) if !s.is_empty() => Ok(str_val(s)),
        Ok(_) => Ok(notp_err(span, "invalid totp handle")),
        Err(e) => Ok(e),
    }
}

fn notp_totp_verify_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "totp.verify", span)?;
    let id = handle_id_from_arg(args, 0, span, "totp.verify")?;
    let token = string_arg(args, 1, "totp.verify", span)?;
    let time = if args.len() >= 3 {
        let t = int_arg(args, 2, "totp.verify", span)?;
        if t < 0 {
            return Ok(notp_err(span, "unix time must be >= 0"));
        }
        t as u64
    } else {
        system_unix_s()
    };
    let window = optional_int(args, 3, 0) as u64;
    match with_handle(id, span, |h| {
        if let OtpHandle::Totp(t) = h {
            t.verify(&token, time, window)
        } else {
            false
        }
    })? {
        Ok(b) => Ok(bool_val(b)),
        Err(e) => Ok(e),
    }
}

fn notp_totp_provisioning_uri_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "totp.provisioning_uri", span)?;
    let id = handle_id_from_arg(args, 0, span, "totp.provisioning_uri")?;
    let name = string_arg(args, 1, "totp.provisioning_uri", span)?;
    let issuer = if args.len() >= 3 {
        Some(string_arg(args, 2, "totp.provisioning_uri", span)?)
    } else {
        None
    };
    match with_handle(id, span, |h| {
        if let OtpHandle::Totp(t) = h {
            t.provisioning_uri(&name, issuer.as_deref())
        } else {
            String::new()
        }
    })? {
        Ok(s) if !s.is_empty() => Ok(str_val(s)),
        Ok(_) => Ok(notp_err(span, "invalid totp handle")),
        Err(e) => Ok(e),
    }
}

fn notp_hotp_at_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "hotp.at", span)?;
    let id = handle_id_from_arg(args, 0, span, "hotp.at")?;
    let counter = int_arg(args, 1, "hotp.at", span)?;
    if counter < 0 {
        return Ok(notp_err(span, "counter must be >= 0"));
    }
    match with_handle(id, span, |h| {
        if let OtpHandle::Hotp(hotp) = h {
            hotp.at(counter as u64)
        } else {
            String::new()
        }
    })? {
        Ok(s) if !s.is_empty() => Ok(str_val(s)),
        Ok(_) => Ok(notp_err(span, "invalid hotp handle")),
        Err(e) => Ok(e),
    }
}

fn notp_hotp_verify_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "hotp.verify", span)?;
    let id = handle_id_from_arg(args, 0, span, "hotp.verify")?;
    let token = string_arg(args, 1, "hotp.verify", span)?;
    let counter = int_arg(args, 2, "hotp.verify", span)?;
    if counter < 0 {
        return Ok(notp_err(span, "counter must be >= 0"));
    }
    match with_handle(id, span, |h| {
        if let OtpHandle::Hotp(hotp) = h {
            hotp.verify(&token, counter as u64)
        } else {
            false
        }
    })? {
        Ok(b) => Ok(bool_val(b)),
        Err(e) => Ok(e),
    }
}

fn notp_hotp_verify_window_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "hotp.verify_window", span)?;
    let id = handle_id_from_arg(args, 0, span, "hotp.verify_window")?;
    let token = string_arg(args, 1, "hotp.verify_window", span)?;
    let counter = int_arg(args, 2, "hotp.verify_window", span)?;
    if counter < 0 {
        return Ok(notp_err(span, "counter must be >= 0"));
    }
    let window = optional_int(args, 3, 0) as u64;
    match with_handle(id, span, |h| {
        if let OtpHandle::Hotp(hotp) = h {
            hotp.verify_window(&token, counter as u64, window)
        } else {
            None
        }
    })? {
        Ok(Some(c)) => Ok(int_val(c as i64)),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn notp_hotp_provisioning_uri_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "hotp.provisioning_uri", span)?;
    let id = handle_id_from_arg(args, 0, span, "hotp.provisioning_uri")?;
    let name = string_arg(args, 1, "hotp.provisioning_uri", span)?;
    let issuer = if args.len() >= 3 {
        Some(string_arg(args, 2, "hotp.provisioning_uri", span)?)
    } else {
        None
    };
    let counter = if args.len() >= 4 {
        let c = int_arg(args, 3, "hotp.provisioning_uri", span)?;
        if c < 0 {
            return Ok(notp_err(span, "counter must be >= 0"));
        }
        Some(c as u64)
    } else {
        None
    };
    match with_handle(id, span, |h| {
        if let OtpHandle::Hotp(hotp) = h {
            hotp.provisioning_uri(&name, issuer.as_deref(), counter)
        } else {
            String::new()
        }
    })? {
        Ok(s) if !s.is_empty() => Ok(str_val(s)),
        Ok(_) => Ok(notp_err(span, "invalid hotp handle")),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! notp_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

notp_fns![
    ("notp_random_base32", "random_base32", notp_random_base32),
    ("notp_compare", "compare", notp_compare),
    ("notp_base32_decode", "base32_decode", notp_base32_decode),
    ("notp_base32_encode", "base32_encode", notp_base32_encode),
    ("notp_totp", "totp", notp_totp),
    ("notp_hotp", "hotp", notp_hotp),
    ("notp_totp_at", "totp_at", notp_totp_at),
    ("notp_hotp_at", "hotp_at", notp_hotp_at),
    ("notp_totp_now", "totp_now", notp_totp_now),
    ("notp_parse_uri", "parse_uri", notp_parse_uri),
    ("notp_totp_at_bulk", "totp_at_bulk", notp_totp_at_bulk),
    ("notp_hotp_at_bulk", "hotp_at_bulk", notp_hotp_at_bulk),
];

pub const MODULE_NAME: &str = "notp";
pub const MODULE_PATHS: &[&str] = &["notp", "std/notp"];

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
    map.insert("DEFAULT_DIGITS".to_string(), int_val(DEFAULT_DIGITS as i64));
    map.insert(
        "DEFAULT_INTERVAL".to_string(),
        int_val(DEFAULT_INTERVAL as i64),
    );
    map.insert("MIN_DIGITS".to_string(), int_val(MIN_DIGITS as i64));
    map.insert("MAX_DIGITS".to_string(), int_val(MAX_DIGITS as i64));
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
    fn hotp_rfc_vector() {
        let code = notp_hotp_at(
            &[
                Value::String("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into()).ref_cell(),
                Value::Int(0).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*code.borrow() {
            Value::String(s) => assert_eq!(s, "755224"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn totp_at_59() {
        let code = notp_totp_at(
            &[
                Value::String("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into()).ref_cell(),
                Value::Int(59).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*code.borrow() {
            Value::String(s) => assert_eq!(s, "287082"),
            other => panic!("expected string, got {other:?}"),
        }
    }
}
