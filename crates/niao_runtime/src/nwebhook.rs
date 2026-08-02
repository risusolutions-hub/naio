//! Native `nwebhook` standard library — Standard Webhooks HMAC sign/verify,
//! timestamps, replay defense. ~svix / standard-webhooks subset.
//!
//! Import with `import "nwebhook"` (or `import "std/nwebhook"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_json_core::Value as JsonValue;
use niao_webhook::{
    check_timestamp, encode_secret, make_headers, new_msg_id, now_secs, parse_secret,
    parse_timestamp, sign_request, ReplayGuard, SecretFormat, SignRequest, Verified,
    VerifyOptions, Webhook, WebhookError, WebhookOptions, DEFAULT_TOLERANCE_SECS, HDR_ID,
    HDR_SIGNATURE, HDR_TIMESTAMP, SECRET_PREFIX,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4450: u32 = codes::E4460_NWEBHOOK_ARITY;
const E4451: u32 = codes::E4461_NWEBHOOK_ERROR;
const E4452: u32 = codes::E4462_NWEBHOOK_TYPE;
const E4453: u32 = codes::E4463_NWEBHOOK_INVALID_HANDLE;

// ---------------------------------------------------------------------------
// Handles
// ---------------------------------------------------------------------------

enum NwebhookHandle {
    Webhook(Webhook),
    Guard(ReplayGuard),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, NwebhookHandle>> = RefCell::new(HashMap::new());
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

fn register(handle: NwebhookHandle) -> i64 {
    let id = new_handle();
    HANDLES.with(|m| m.borrow_mut().insert(id, handle));
    id
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut NwebhookHandle) -> T,
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
    RuntimeError::at(span, E4452, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4450,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4450,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn wh_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4451, "nwebhook_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4453,
        "nwebhook_error",
        format!("invalid or closed nwebhook handle {id}"),
        span,
    )
}

fn map_err(span: Span, e: WebhookError) -> ValueRef {
    wh_err(span, e.to_string())
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

fn obj_str(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str) -> Option<i64> {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn secret_format(map: &HashMap<String, ValueRef>) -> SecretFormat {
    match obj_str(map, "format").as_deref() {
        Some("raw") => SecretFormat::Raw,
        _ => SecretFormat::Standard,
    }
}

fn verify_opts_from(map: &HashMap<String, ValueRef>) -> VerifyOptions {
    VerifyOptions {
        tolerance: obj_int(map, "tolerance").unwrap_or(DEFAULT_TOLERANCE_SECS),
        now: obj_int(map, "now"),
        parse_json: obj_bool(map, "parse_json", true),
    }
}

fn headers_from_value(v: &Value, span: Span, name: &str) -> NiaoResult<HashMap<String, String>> {
    match v {
        Value::Object(map) => {
            let mut out = HashMap::new();
            for (k, vr) in map {
                match &*vr.borrow() {
                    Value::String(s) => {
                        out.insert(k.clone(), s.clone());
                    }
                    Value::Int(n) => {
                        out.insert(k.clone(), n.to_string());
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() header values must be string/int, got {}",
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
            format!("{name}() expects headers object, got {}", other.type_name()),
        )),
    }
}

fn from_json(v: JsonValue) -> ValueRef {
    match v {
        JsonValue::Null => Value::Nil.ref_cell(),
        JsonValue::Bool(b) => Value::Bool(b).ref_cell(),
        JsonValue::Number(n) => match n {
            niao_json_core::Number::I64(i) => Value::Int(i).ref_cell(),
            niao_json_core::Number::U64(u) if u <= i64::MAX as u64 => {
                Value::Int(u as i64).ref_cell()
            }
            niao_json_core::Number::U64(u) => Value::String(u.to_string()).ref_cell(),
            niao_json_core::Number::F64(f) => Value::Float(f).ref_cell(),
        },
        JsonValue::String(s) => Value::String(s).ref_cell(),
        JsonValue::Array(items) => {
            let out: Vec<ValueRef> = items.into_iter().map(from_json).collect();
            Value::Array(out).ref_cell()
        }
        JsonValue::Object(map) => {
            let mut out: HashMap<String, ValueRef> = HashMap::new();
            for (k, v) in map.iter() {
                out.insert(k.to_string(), from_json(v.clone()));
            }
            Value::Object(out).ref_cell()
        }
    }
}

fn verified_to_value(v: Verified, include_meta: bool) -> ValueRef {
    if include_meta {
        let mut map = HashMap::new();
        map.insert("id".to_string(), str_val(v.id));
        map.insert("timestamp".to_string(), int_val(v.timestamp));
        map.insert("payload".to_string(), str_val(v.payload));
        match v.json {
            Some(j) => {
                map.insert("data".to_string(), from_json(j));
            }
            None => {
                map.insert("data".to_string(), Value::Nil.ref_cell());
            }
        }
        Value::Object(map).ref_cell()
    } else {
        match v.json {
            Some(j) => from_json(j),
            None => Value::Nil.ref_cell(),
        }
    }
}

fn headers_to_value(h: &HashMap<String, String>) -> ValueRef {
    let mut map = HashMap::new();
    for (k, v) in h {
        map.insert(k.clone(), str_val(v.clone()));
    }
    Value::Object(map).ref_cell()
}

fn sign_request_to_value(r: SignRequest) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("id".to_string(), str_val(r.id));
    map.insert("timestamp".to_string(), int_val(r.timestamp));
    map.insert("signature".to_string(), str_val(r.signature));
    map.insert("headers".to_string(), headers_to_value(&r.headers));
    map.insert("payload".to_string(), str_val(r.payload));
    Value::Object(map).ref_cell()
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
            None => Err(type_err(
                span,
                format!("{name}() expects nwebhook handle object"),
            )),
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

fn make_webhook(secret_arg: &Value, opts: &HashMap<String, ValueRef>) -> Result<Webhook, WebhookError> {
    let format = secret_format(opts);
    match secret_arg {
        Value::String(s) => Webhook::new(s, WebhookOptions { format }),
        Value::Array(items) => {
            let mut secrets = Vec::with_capacity(items.len());
            let mut owned = Vec::with_capacity(items.len());
            for it in items {
                match &*it.borrow() {
                    Value::String(s) => owned.push(s.clone()),
                    _ => {
                        return Err(WebhookError::BadArgument(
                            "secrets array must contain strings".into(),
                        ))
                    }
                }
            }
            for s in &owned {
                secrets.push(s.as_str());
            }
            Webhook::with_secrets(&secrets, format)
        }
        _ => Err(WebhookError::BadArgument(
            "secret must be a string or array of strings".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// webhook(secret, opts?) handle
// ---------------------------------------------------------------------------

// >>> nwebhook.webhook("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw").sign("msg_p5jXN8AQM9LWM0D4loKWxJek", 1614265330, "{\"test\": 2432232314}")
// "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE="
fn nwebhook_webhook(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwebhook_webhook", span)?;
    let opts = parse_opts(args, 1, span)?;
    match make_webhook(&*args[0].borrow(), &opts) {
        Ok(wh) => {
            let id = register(NwebhookHandle::Webhook(wh));
            let mut methods = HashMap::new();
            methods.insert(
                "sign".to_string(),
                Value::NativeFunction(Rc::new(nwebhook_wh_sign_method)).ref_cell(),
            );
            methods.insert(
                "verify".to_string(),
                Value::NativeFunction(Rc::new(nwebhook_wh_verify_method)).ref_cell(),
            );
            methods.insert(
                "verify_raw".to_string(),
                Value::NativeFunction(Rc::new(nwebhook_wh_verify_raw_method)).ref_cell(),
            );
            methods.insert(
                "valid".to_string(),
                Value::NativeFunction(Rc::new(nwebhook_wh_valid_method)).ref_cell(),
            );
            Ok(handle_object(id, "webhook", methods))
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nwebhook_wh_sign_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "webhook.sign", span)?;
    let id = handle_id_from_arg(args, 0, span, "webhook.sign")?;
    let msg_id = string_arg(args, 1, "webhook.sign", span)?;
    let ts = int_arg(args, 2, "webhook.sign", span)?;
    let payload = string_arg(args, 3, "webhook.sign", span)?;
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Webhook(w) = h {
            w.sign(&msg_id, ts, &payload)
        } else {
            Err(WebhookError::BadArgument("not a webhook handle".into()))
        }
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nwebhook_wh_verify_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "webhook.verify", span)?;
    let id = handle_id_from_arg(args, 0, span, "webhook.verify")?;
    let payload = string_arg(args, 1, "webhook.verify", span)?;
    let headers = headers_from_value(&*args[2].borrow(), span, "webhook.verify")?;
    let opts_map = parse_opts(args, 3, span)?;
    let vopts = verify_opts_from(&opts_map);
    let meta = obj_bool(&opts_map, "meta", false);
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Webhook(w) = h {
            w.verify(&payload, &headers, &vopts)
        } else {
            Err(WebhookError::BadArgument("not a webhook handle".into()))
        }
    })? {
        Ok(Ok(v)) => Ok(verified_to_value(v, meta)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nwebhook_wh_verify_raw_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "webhook.verify_raw", span)?;
    let id = handle_id_from_arg(args, 0, span, "webhook.verify_raw")?;
    let payload = string_arg(args, 1, "webhook.verify_raw", span)?;
    let headers = headers_from_value(&*args[2].borrow(), span, "webhook.verify_raw")?;
    let opts_map = parse_opts(args, 3, span)?;
    let mut vopts = verify_opts_from(&opts_map);
    vopts.parse_json = false;
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Webhook(w) = h {
            w.verify_raw(&payload, &headers, &vopts)
        } else {
            Err(WebhookError::BadArgument("not a webhook handle".into()))
        }
    })? {
        Ok(Ok(v)) => Ok(str_val(v.payload)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

fn nwebhook_wh_valid_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "webhook.valid", span)?;
    let id = handle_id_from_arg(args, 0, span, "webhook.valid")?;
    let payload = string_arg(args, 1, "webhook.valid", span)?;
    let headers = headers_from_value(&*args[2].borrow(), span, "webhook.valid")?;
    let opts_map = parse_opts(args, 3, span)?;
    let mut vopts = verify_opts_from(&opts_map);
    vopts.parse_json = false;
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Webhook(w) = h {
            Ok(w.valid(&payload, &headers, &vopts))
        } else {
            Ok(false)
        }
    })? {
        Ok(Ok(b)) => Ok(bool_val(b)),
        Ok(Err(e)) => Ok(map_err(span, e)),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// One-shot module functions
// ---------------------------------------------------------------------------

// >>> nwebhook.sign("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "msg_p5jXN8AQM9LWM0D4loKWxJek", 1614265330, "{\"test\": 2432232314}")
// "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE="
fn nwebhook_sign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 5, "nwebhook_sign", span)?;
    let opts = parse_opts(args, 4, span)?;
    let msg_id = string_arg(args, 1, "nwebhook_sign", span)?;
    let ts = int_arg(args, 2, "nwebhook_sign", span)?;
    let payload = string_arg(args, 3, "nwebhook_sign", span)?;
    match make_webhook(&*args[0].borrow(), &opts) {
        Ok(wh) => match wh.sign(&msg_id, ts, &payload) {
            Ok(s) => Ok(str_val(s)),
            Err(e) => Ok(map_err(span, e)),
        },
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> type(nwebhook.verify("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "{\"test\": 2432232314}", nwebhook.headers("msg_p5jXN8AQM9LWM0D4loKWxJek", 1614265330, nwebhook.sign("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "msg_p5jXN8AQM9LWM0D4loKWxJek", 1614265330, "{\"test\": 2432232314}")), {now: 1614265330}).test)
// "int"
fn nwebhook_verify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nwebhook_verify", span)?;
    let payload = string_arg(args, 1, "nwebhook_verify", span)?;
    let headers = headers_from_value(&*args[2].borrow(), span, "nwebhook_verify")?;
    let opts_map = parse_opts(args, 3, span)?;
    let vopts = verify_opts_from(&opts_map);
    let meta = obj_bool(&opts_map, "meta", false);
    match make_webhook(&*args[0].borrow(), &opts_map) {
        Ok(wh) => match wh.verify(&payload, &headers, &vopts) {
            Ok(v) => Ok(verified_to_value(v, meta)),
            Err(e) => Ok(map_err(span, e)),
        },
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nwebhook.verify_raw("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "hello", nwebhook.headers("msg_x", 1614265330, nwebhook.sign("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "msg_x", 1614265330, "hello")), {now: 1614265330})
// "hello"
fn nwebhook_verify_raw(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nwebhook_verify_raw", span)?;
    let payload = string_arg(args, 1, "nwebhook_verify_raw", span)?;
    let headers = headers_from_value(&*args[2].borrow(), span, "nwebhook_verify_raw")?;
    let opts_map = parse_opts(args, 3, span)?;
    let mut vopts = verify_opts_from(&opts_map);
    vopts.parse_json = false;
    match make_webhook(&*args[0].borrow(), &opts_map) {
        Ok(wh) => match wh.verify_raw(&payload, &headers, &vopts) {
            Ok(v) => Ok(str_val(v.payload)),
            Err(e) => Ok(map_err(span, e)),
        },
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nwebhook.valid("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "x", nwebhook.headers("m", 1614265330, nwebhook.sign("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "m", 1614265330, "x")), {now: 1614265330})
// true
fn nwebhook_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nwebhook_valid", span)?;
    let payload = string_arg(args, 1, "nwebhook_valid", span)?;
    let headers = headers_from_value(&*args[2].borrow(), span, "nwebhook_valid")?;
    let opts_map = parse_opts(args, 3, span)?;
    let mut vopts = verify_opts_from(&opts_map);
    vopts.parse_json = false;
    match make_webhook(&*args[0].borrow(), &opts_map) {
        Ok(wh) => Ok(bool_val(wh.valid(&payload, &headers, &vopts))),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nwebhook.sign_request("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw", "{\"ok\":true}", {id: "msg_fixed", timestamp: 1614265330}).signature
// "v1,…"
fn nwebhook_sign_request(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nwebhook_sign_request", span)?;
    let payload = string_arg(args, 1, "nwebhook_sign_request", span)?;
    let opts = parse_opts(args, 2, span)?;
    match make_webhook(&*args[0].borrow(), &opts) {
        Ok(wh) => {
            let id = obj_str(&opts, "id");
            let ts = obj_int(&opts, "timestamp");
            match sign_request(&wh, &payload, id.as_deref(), ts) {
                Ok(r) => Ok(sign_request_to_value(r)),
                Err(e) => Ok(map_err(span, e)),
            }
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nwebhook.headers("msg_a", 1, "v1,abc")["webhook-id"]
// "msg_a"
fn nwebhook_headers(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nwebhook_headers", span)?;
    let id = string_arg(args, 0, "nwebhook_headers", span)?;
    let ts = int_arg(args, 1, "nwebhook_headers", span)?;
    let sig = string_arg(args, 2, "nwebhook_headers", span)?;
    Ok(headers_to_value(&make_headers(&id, ts, &sig)))
}

// >>> nwebhook.new_id().starts_with("msg_")
// true
fn nwebhook_new_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nwebhook_new_id", span)?;
    Ok(str_val(new_msg_id()))
}

// >>> nwebhook.now() > 0
// true
fn nwebhook_now(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nwebhook_now", span)?;
    Ok(int_val(now_secs()))
}

// >>> nwebhook.parse_secret("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw").ok
// true
fn nwebhook_parse_secret(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwebhook_parse_secret", span)?;
    let secret = string_arg(args, 0, "nwebhook_parse_secret", span)?;
    let opts = parse_opts(args, 1, span)?;
    let format = secret_format(&opts);
    match parse_secret(&secret, format) {
        Ok(bytes) => {
            let mut map = HashMap::new();
            map.insert("ok".to_string(), bool_val(true));
            map.insert("len".to_string(), int_val(bytes.len() as i64));
            map.insert(
                "encoded".to_string(),
                str_val(encode_secret(&bytes)),
            );
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nwebhook.check_timestamp(1614265330, {now: 1614265330, tolerance: 300})
// true
fn nwebhook_check_timestamp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nwebhook_check_timestamp", span)?;
    let opts = parse_opts(args, 1, span)?;
    let now = obj_int(&opts, "now").unwrap_or_else(now_secs);
    let tol = obj_int(&opts, "tolerance").unwrap_or(DEFAULT_TOLERANCE_SECS);
    let ts = match &*args[0].borrow() {
        Value::Int(n) => *n,
        Value::String(s) => match parse_timestamp(s) {
            Ok(n) => n,
            Err(e) => return Ok(map_err(span, e)),
        },
        other => {
            return Err(type_err(
                span,
                format!(
                    "nwebhook_check_timestamp() expects int or string, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    match check_timestamp(ts, now, tol) {
        Ok(()) => Ok(bool_val(true)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Replay guard
// ---------------------------------------------------------------------------

// >>> let g = nwebhook.guard(); g.check("msg_1")
// true
fn nwebhook_guard(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nwebhook_guard", span)?;
    let opts = parse_opts(args, 0, span)?;
    let max_age = obj_int(&opts, "max_age").unwrap_or(DEFAULT_TOLERANCE_SECS);
    let capacity = obj_int(&opts, "capacity").unwrap_or(10_000) as usize;
    let id = register(NwebhookHandle::Guard(ReplayGuard::new(max_age, capacity)));
    let mut methods = HashMap::new();
    methods.insert(
        "check".to_string(),
        Value::NativeFunction(Rc::new(nwebhook_guard_check_method)).ref_cell(),
    );
    methods.insert(
        "seen".to_string(),
        Value::NativeFunction(Rc::new(nwebhook_guard_seen_method)).ref_cell(),
    );
    methods.insert(
        "forget".to_string(),
        Value::NativeFunction(Rc::new(nwebhook_guard_forget_method)).ref_cell(),
    );
    methods.insert(
        "clear".to_string(),
        Value::NativeFunction(Rc::new(nwebhook_guard_clear_method)).ref_cell(),
    );
    methods.insert(
        "size".to_string(),
        Value::NativeFunction(Rc::new(nwebhook_guard_size_method)).ref_cell(),
    );
    methods.insert("max_age".to_string(), int_val(max_age));
    methods.insert("capacity".to_string(), int_val(capacity as i64));
    Ok(handle_object(id, "guard", methods))
}

fn guard_now(args: &[ValueRef], idx: usize) -> i64 {
    if args.len() > idx {
        match &*args[idx].borrow() {
            Value::Int(n) => return *n,
            Value::Object(map) => {
                if let Some(n) = obj_int(map, "now") {
                    return n;
                }
            }
            _ => {}
        }
    }
    now_secs()
}

fn nwebhook_guard_check_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "guard.check", span)?;
    let id = handle_id_from_arg(args, 0, span, "guard.check")?;
    let msg_id = string_arg(args, 1, "guard.check", span)?;
    let now = guard_now(args, 2);
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Guard(g) = h {
            g.check(&msg_id, now)
        } else {
            false
        }
    })? {
        Ok(b) => Ok(bool_val(b)),
        Err(e) => Ok(e),
    }
}

fn nwebhook_guard_seen_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "guard.seen", span)?;
    let id = handle_id_from_arg(args, 0, span, "guard.seen")?;
    let msg_id = string_arg(args, 1, "guard.seen", span)?;
    let now = guard_now(args, 2);
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Guard(g) = h {
            g.seen(&msg_id, now)
        } else {
            false
        }
    })? {
        Ok(b) => Ok(bool_val(b)),
        Err(e) => Ok(e),
    }
}

fn nwebhook_guard_forget_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "guard.forget", span)?;
    let id = handle_id_from_arg(args, 0, span, "guard.forget")?;
    let msg_id = string_arg(args, 1, "guard.forget", span)?;
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Guard(g) = h {
            g.forget(&msg_id);
        }
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nwebhook_guard_clear_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "guard.clear", span)?;
    let id = handle_id_from_arg(args, 0, span, "guard.clear")?;
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Guard(g) = h {
            g.clear();
        }
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nwebhook_guard_size_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "guard.size", span)?;
    let id = handle_id_from_arg(args, 0, span, "guard.size")?;
    match with_handle(id, span, |h| {
        if let NwebhookHandle::Guard(g) = h {
            g.size() as i64
        } else {
            0
        }
    })? {
        Ok(n) => Ok(int_val(n)),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nwebhook_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nwebhook_fns![
    ("nwebhook_webhook", "webhook", nwebhook_webhook),
    ("nwebhook_guard", "guard", nwebhook_guard),
    ("nwebhook_sign", "sign", nwebhook_sign),
    ("nwebhook_verify", "verify", nwebhook_verify),
    ("nwebhook_verify_raw", "verify_raw", nwebhook_verify_raw),
    ("nwebhook_valid", "valid", nwebhook_valid),
    ("nwebhook_sign_request", "sign_request", nwebhook_sign_request),
    ("nwebhook_headers", "headers", nwebhook_headers),
    ("nwebhook_new_id", "new_id", nwebhook_new_id),
    ("nwebhook_now", "now", nwebhook_now),
    ("nwebhook_parse_secret", "parse_secret", nwebhook_parse_secret),
    ("nwebhook_check_timestamp", "check_timestamp", nwebhook_check_timestamp),
];

pub const MODULE_NAME: &str = "nwebhook";
pub const MODULE_PATHS: &[&str] = &["nwebhook", "std/nwebhook"];

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
    map.insert("TOLERANCE".to_string(), int_val(DEFAULT_TOLERANCE_SECS));
    map.insert("SECRET_PREFIX".to_string(), str_val(SECRET_PREFIX));
    map.insert("HDR_ID".to_string(), str_val(HDR_ID));
    map.insert("HDR_TIMESTAMP".to_string(), str_val(HDR_TIMESTAMP));
    map.insert("HDR_SIGNATURE".to_string(), str_val(HDR_SIGNATURE));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn call(f: NativeFn, args: Vec<ValueRef>) -> ValueRef {
        f(&args, span()).unwrap()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn obj(pairs: Vec<(&str, ValueRef)>) -> ValueRef {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v);
        }
        Value::Object(m).ref_cell()
    }

    #[test]
    fn module_exports() {
        let ns = namespace();
        match ns {
            Value::Object(m) => {
                assert!(m.contains_key("sign"));
                assert!(m.contains_key("verify"));
                assert!(m.contains_key("webhook"));
                assert!(m.contains_key("guard"));
                assert!(m.contains_key("TOLERANCE"));
            }
            _ => panic!("expected object namespace"),
        }
    }

    #[test]
    fn doctest_official_sign_vector() {
        let got = call(
            nwebhook_sign,
            vec![
                s("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw"),
                s("msg_p5jXN8AQM9LWM0D4loKWxJek"),
                i(1_614_265_330),
                s(r#"{"test": 2432232314}"#),
            ],
        );
        match &*got.borrow() {
            Value::String(sig) => assert_eq!(
                sig,
                "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE="
            ),
            other => panic!("expected string signature, got {other:?}"),
        }
    }

    #[test]
    fn doctest_new_id_prefix() {
        let id = call(nwebhook_new_id, vec![]);
        match &*id.borrow() {
            Value::String(s) => assert!(s.starts_with("msg_")),
            other => panic!("expected string id, got {other:?}"),
        }
    }

    #[test]
    fn doctest_now_positive() {
        let n = call(nwebhook_now, vec![]);
        match &*n.borrow() {
            Value::Int(v) => assert!(*v > 0),
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn doctest_headers_roundtrip() {
        let hdrs = call(
            nwebhook_headers,
            vec![s("msg_a"), i(1), s("v1,abc")],
        );
        match &*hdrs.borrow() {
            Value::Object(m) => {
                match &*m.get("webhook-id").unwrap().borrow() {
                    Value::String(id) => assert_eq!(id, "msg_a"),
                    other => panic!("bad id: {other:?}"),
                }
            }
            other => panic!("expected headers object, got {other:?}"),
        }
    }

    #[test]
    fn doctest_verify_json_field() {
        let secret = s("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw");
        let msg_id = "msg_p5jXN8AQM9LWM0D4loKWxJek";
        let ts = 1_614_265_330;
        let payload = r#"{"test": 2432232314}"#;
        let sig = call(
            nwebhook_sign,
            vec![secret.clone(), s(msg_id), i(ts), s(payload)],
        );
        let sig_str = match &*sig.borrow() {
            Value::String(v) => v.clone(),
            other => panic!("sig: {other:?}"),
        };
        let headers = call(
            nwebhook_headers,
            vec![s(msg_id), i(ts), s(&sig_str)],
        );
        let data = call(
            nwebhook_verify,
            vec![
                secret,
                s(payload),
                headers,
                obj(vec![("now", i(ts))]),
            ],
        );
        match &*data.borrow() {
            Value::Object(m) => match &*m.get("test").unwrap().borrow() {
                Value::Int(n) => assert_eq!(*n, 2_432_232_314),
                other => panic!("test field: {other:?}"),
            },
            other => panic!("verify data: {other:?}"),
        }
    }

    #[test]
    fn doctest_verify_raw() {
        let secret = s("MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw");
        let msg_id = "msg_x";
        let ts = 1_614_265_330;
        let body = "hello";
        let sig = call(
            nwebhook_sign,
            vec![secret.clone(), s(msg_id), i(ts), s(body)],
        );
        let sig_str = match &*sig.borrow() {
            Value::String(v) => v.clone(),
            other => panic!("sig: {other:?}"),
        };
        let headers = call(
            nwebhook_headers,
            vec![s(msg_id), i(ts), s(&sig_str)],
        );
        let out = call(
            nwebhook_verify_raw,
            vec![
                secret,
                s(body),
                headers,
                obj(vec![("now", i(ts))]),
            ],
        );
        match &*out.borrow() {
            Value::String(v) => assert_eq!(v, "hello"),
            other => panic!("verify_raw: {other:?}"),
        }
    }

    #[test]
    fn doctest_check_timestamp() {
        let ok = call(
            nwebhook_check_timestamp,
            vec![i(1_614_265_330), obj(vec![("now", i(1_614_265_330)), ("tolerance", i(300))])],
        );
        match &*ok.borrow() {
            Value::Bool(b) => assert!(*b),
            other => panic!("check_timestamp: {other:?}"),
        }
    }

    #[test]
    fn doctest_parse_secret() {
        let info = call(
            nwebhook_parse_secret,
            vec![s("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw")],
        );
        match &*info.borrow() {
            Value::Object(m) => match &*m.get("ok").unwrap().borrow() {
                Value::Bool(b) => assert!(*b),
                other => panic!("ok: {other:?}"),
            },
            other => panic!("parse_secret: {other:?}"),
        }
    }
}
