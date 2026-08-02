//! Native nrpc standard library — JSON-RPC 2.0 client/server over stdio, TCP,
//! HTTP (~jsonrpcserver subset).
//!
//! Import with `import "nrpc"` (or `import "std/nrpc"`).

use crate::{
    call_niao_function, error_value, resolve_niao_function_by_name, NativeFn, NiaoResult,
    RuntimeError, Value, ValueRef,
};
use niao_ast::Span;
use niao_bignum::BigInt;
use niao_errors::codes;
use niao_json_core::{Number as JNumber, Object as JObject, Value as JsonValue};
use niao_rpc::{
    codes as rpc_codes, decode, encode_value, frame_text, handle_payload, http_call,
    http_serve_once, parse_message_value, parse_response_value, stdio_exchange, tcp_call,
    tcp_serve_once, unframe, valid, EngineError, FrameStyle, Id, Message, Request, Response,
    ResponseBody, RpcError, TransportOptions, MAX_BYTES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Server / client stores
// ---------------------------------------------------------------------------

struct ServerStore {
    methods: HashMap<String, Handler>,
}

enum Handler {
    Callable(ValueRef),
    Named(String),
}

impl Clone for Handler {
    fn clone(&self) -> Self {
        match self {
            Handler::Callable(v) => Handler::Callable(Rc::clone(v)),
            Handler::Named(s) => Handler::Named(s.clone()),
        }
    }
}

struct ClientStore {
    next_id: i64,
}

thread_local! {
    static SERVERS: RefCell<HashMap<i64, ServerStore>> = RefCell::new(HashMap::new());
    static CLIENTS: RefCell<HashMap<i64, ClientStore>> = RefCell::new(HashMap::new());
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

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4452_NRPC_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4450_NRPC_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    arity_range(args, n, n, name, span)
}

fn nrpc_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4451_NRPC_ERROR, "nrpc_error", msg.into(), span)
}

fn parse_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4453_NRPC_PARSE, "nrpc_error", msg.into(), span)
}

fn invalid_handle(span: Span, kind: &str, id: i64) -> ValueRef {
    error_value(
        codes::E4454_NRPC_INVALID_HANDLE,
        "nrpc_error",
        format!("invalid or closed {kind} handle {id}"),
        span,
    )
}

fn map_engine(span: Span, e: EngineError) -> ValueRef {
    match e {
        EngineError::Parse(m) | EngineError::Invalid(m) => parse_err(span, m),
        other => nrpc_err(span, other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Arg helpers + JSON bridge
// ---------------------------------------------------------------------------

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

fn text_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        Value::ByteArray(b) => String::from_utf8(b.clone()).map_err(|_| {
            type_err(span, format!("{name}() byte[] argument must be valid UTF-8"))
        }),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or byte[] as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn json_to_niao(j: JsonValue) -> Value {
    match j {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => match n {
            JNumber::I64(i) => Value::Int(i),
            JNumber::U64(u) if u <= i64::MAX as u64 => Value::Int(u as i64),
            JNumber::U64(u) => Value::BigInt(BigInt::from(u)),
            JNumber::F64(f) => {
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Value::Int(f as i64)
                } else {
                    Value::Float(f)
                }
            }
        },
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(items) => {
            Value::Array(items.into_iter().map(|i| json_to_niao(i).ref_cell()).collect())
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (k, v) in map.iter() {
                out.insert(k.to_string(), json_to_niao(v.clone()).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn niao_to_json(v: &Value, span: Span) -> NiaoResult<JsonValue> {
    match v {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(n) => Ok(JsonValue::Number(JNumber::I64(*n))),
        Value::BigInt(n) => {
            if let Some(i) = n.to_i64() {
                Ok(JsonValue::Number(JNumber::I64(i)))
            } else if let Some(u) = n.to_u64() {
                Ok(JsonValue::Number(JNumber::U64(u)))
            } else {
                Err(type_err(
                    span,
                    format!("bigint {n} does not fit in a JSON number"),
                ))
            }
        }
        Value::Float(f) => {
            if !f.is_finite() {
                Ok(JsonValue::Null)
            } else {
                Ok(JsonValue::Number(JNumber::F64(*f)))
            }
        }
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for slot in items {
                out.push(niao_to_json(&slot.borrow(), span)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::Object(map) => {
            let mut out = JObject::with_capacity(map.len());
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                out.insert(k.clone(), niao_to_json(&map[k].borrow(), span)?);
            }
            Ok(JsonValue::Object(out))
        }
        Value::IntArray(items) => Ok(JsonValue::Array(
            items
                .iter()
                .map(|&n| JsonValue::Number(JNumber::I64(n)))
                .collect(),
        )),
        Value::FloatArray(items) => Ok(JsonValue::Array(
            items
                .iter()
                .map(|&f| {
                    if f.is_finite() {
                        JsonValue::Number(JNumber::F64(f))
                    } else {
                        JsonValue::Null
                    }
                })
                .collect(),
        )),
        Value::BoolArray(items) => Ok(JsonValue::Array(
            items.iter().map(|&b| JsonValue::Bool(b != 0)).collect(),
        )),
        Value::StringArray(items) => {
            let mut out = Vec::with_capacity(items.len());
            for i in 0..items.len() {
                out.push(JsonValue::String(items.get(i).unwrap_or_default()));
            }
            Ok(JsonValue::Array(out))
        }
        Value::ByteArray(items) => Ok(JsonValue::Array(
            items
                .iter()
                .map(|&b| JsonValue::Number(JNumber::I64(b as i64)))
                .collect(),
        )),
        other => Err(type_err(
            span,
            format!("cannot convert {} to JSON-RPC value", other.type_name()),
        )),
    }
}

fn id_from_niao(v: &Value, span: Span) -> NiaoResult<Id> {
    match v {
        Value::Nil => Ok(Id::Null),
        Value::Int(n) => Ok(Id::Number(*n)),
        Value::String(s) => Ok(Id::String(s.clone())),
        other => Err(type_err(
            span,
            format!("id must be int, string, or nil, got {}", other.type_name()),
        )),
    }
}

fn id_to_niao(id: &Id) -> Value {
    match id {
        Id::Null => Value::Nil,
        Id::Number(n) => Value::Int(*n),
        Id::String(s) => Value::String(s.clone()),
    }
}

fn response_to_niao(r: &Response) -> Value {
    json_to_niao(r.to_value())
}

fn request_to_niao(r: &Request) -> Value {
    json_to_niao(r.to_value())
}

fn style_from_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<FrameStyle> {
    if idx >= args.len() {
        return Ok(FrameStyle::Ndjson);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(FrameStyle::Ndjson),
        Value::String(s) => FrameStyle::parse(s).map_err(|e| type_err(span, e.to_string())),
        Value::Object(map) => {
            if let Some(v) = map.get("style") {
                match &*v.borrow() {
                    Value::String(s) => {
                        FrameStyle::parse(s).map_err(|e| type_err(span, e.to_string()))
                    }
                    _ => Ok(FrameStyle::Ndjson),
                }
            } else {
                Ok(FrameStyle::Ndjson)
            }
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() frame style must be string or options object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn transport_opts_from(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<TransportOptions> {
    let mut opts = TransportOptions::default();
    if idx >= args.len() {
        return Ok(opts);
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(opts),
        Value::String(s) => {
            opts.style = FrameStyle::parse(s).map_err(|e| type_err(span, e.to_string()))?;
            Ok(opts)
        }
        Value::Object(map) => {
            if let Some(v) = map.get("style") {
                if let Value::String(s) = &*v.borrow() {
                    opts.style = FrameStyle::parse(s).map_err(|e| type_err(span, e.to_string()))?;
                }
            }
            if let Some(v) = map.get("timeout_ms") {
                if let Value::Int(n) = &*v.borrow() {
                    if *n < 0 {
                        return Err(type_err(span, "timeout_ms must be non-negative"));
                    }
                    opts.timeout = Some(Duration::from_millis(*n as u64));
                }
            }
            if let Some(v) = map.get("max_requests") {
                if let Value::Int(n) = &*v.borrow() {
                    if *n <= 0 {
                        return Err(type_err(span, "max_requests must be positive"));
                    }
                    opts.max_requests = *n as usize;
                }
            }
            Ok(opts)
        }
        other => Err(type_err(
            span,
            format!(
                "transport options must be object or string, got {}",
                other.type_name()
            ),
        )),
    }
}

fn err_marker(code: i64, message: String, data: Option<JsonValue>) -> Value {
    let mut map = HashMap::new();
    map.insert("__nrpc_error".into(), Value::Bool(true).ref_cell());
    map.insert("code".into(), Value::Int(code).ref_cell());
    map.insert("message".into(), Value::String(message).ref_cell());
    if let Some(d) = data {
        map.insert("data".into(), json_to_niao(d).ref_cell());
    }
    Value::Object(map)
}

fn extract_handler_result(v: &Value, span: Span) -> NiaoResult<Result<JsonValue, RpcError>> {
    if let Value::Object(map) = v {
        if let Some(flag) = map.get("__nrpc_error") {
            if matches!(&*flag.borrow(), Value::Bool(true)) {
                let code = match map.get("code") {
                    Some(c) => match &*c.borrow() {
                        Value::Int(n) => *n,
                        _ => {
                            return Err(type_err(span, "nrpc error marker needs int code"));
                        }
                    },
                    None => rpc_codes::INTERNAL_ERROR,
                };
                let message = match map.get("message") {
                    Some(m) => match &*m.borrow() {
                        Value::String(s) => s.clone(),
                        _ => RpcError::default_message(code).to_string(),
                    },
                    None => RpcError::default_message(code).to_string(),
                };
                let data = match map.get("data") {
                    Some(d) => Some(niao_to_json(&d.borrow(), span)?),
                    None => None,
                };
                return Ok(Err(RpcError { code, message, data }));
            }
        }
    }
    Ok(Ok(niao_to_json(v, span)?))
}

fn call_method(
    handler: &Handler,
    params: Option<&JsonValue>,
    span: Span,
) -> NiaoResult<Result<JsonValue, RpcError>> {
    let callable = match handler {
        Handler::Callable(v) => Rc::clone(v),
        Handler::Named(name) => match resolve_niao_function_by_name(name) {
            Some(v) => v,
            None => {
                return Ok(Err(RpcError::internal_error(format!(
                    "handler function '{name}' not found"
                ))));
            }
        },
    };
    let args = match params {
        None => vec![Value::Nil.ref_cell()],
        Some(p) => vec![json_to_niao(p.clone()).ref_cell()],
    };
    let ok_callable = matches!(
        &*callable.borrow(),
        Value::Function(_) | Value::NativeFunction(_)
    );
    if !ok_callable {
        let ty = callable.borrow().type_name().to_string();
        return Err(type_err(
            span,
            format!("method handler must be callable, got {ty}"),
        ));
    }
    let result = call_niao_function(callable, &args, span)?;
    // Catchable Niao errors from the handler become internal errors.
    let borrowed = result.borrow();
    if let Value::Error(e) = &*borrowed {
        return Ok(Err(RpcError::internal_error(e.message.clone())));
    }
    extract_handler_result(&borrowed, span)
}

fn server_dispatch_str(server_id: i64, input: &str, span: Span) -> NiaoResult<ValueRef> {
    let exists = SERVERS.with(|m| m.borrow().contains_key(&server_id));
    if !exists {
        return Ok(invalid_handle(span, "server", server_id));
    }

    // Collect method names first so we can look up handlers inside the callback
    // without holding the SERVERS borrow across call_niao_function.
    let out = handle_payload(input, |method, params| {
        let handler = SERVERS.with(|m| {
            m.borrow()
                .get(&server_id)
                .and_then(|s| s.methods.get(method).cloned())
        });
        match handler {
            None => Err(RpcError::method_not_found(method)),
            Some(h) => match call_method(&h, params, span) {
                Ok(r) => r,
                Err(re) => Err(RpcError::internal_error(re.message())),
            },
        }
    });
    if out.is_empty() {
        Ok(Value::Nil.ref_cell())
    } else {
        match decode(&out) {
            Ok(msg) => Ok(json_to_niao(msg.to_value()).ref_cell()),
            Err(_) => {
                // handle_payload returns encoded JSON; parse raw
                match niao_json_core::parse(&out) {
                    Ok(v) => Ok(json_to_niao(v).ref_cell()),
                    Err(e) => Ok(parse_err(span, e.to_string())),
                }
            }
        }
    }
}

fn server_call_fn(
    server_id: i64,
    span: Span,
) -> impl FnMut(&str, Option<&JsonValue>) -> Result<JsonValue, RpcError> {
    move |method, params| {
        let handler = SERVERS.with(|m| {
            m.borrow()
                .get(&server_id)
                .and_then(|s| s.methods.get(method).cloned())
        });
        match handler {
            None => Err(RpcError::method_not_found(method)),
            Some(h) => match call_method(&h, params, span) {
                Ok(r) => r,
                Err(re) => Err(RpcError::internal_error(re.message())),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> type(nrpc.request("ping", nil, 1))
fn nrpc_request(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nrpc_request", span)?;
    let method = string_arg(args, 0, "nrpc_request", span)?;
    let params = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Nil => None,
            other => Some(niao_to_json(other, span)?),
        }
    } else {
        None
    };
    let id = if args.len() >= 3 {
        id_from_niao(&args[2].borrow(), span)?
    } else {
        Id::Number(1)
    };
    Ok(request_to_niao(&Request::call(method, params, id)).ref_cell())
}

// >>> nrpc.notify("log", {msg: "hi"}).method
fn nrpc_notify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nrpc_notify", span)?;
    let method = string_arg(args, 0, "nrpc_notify", span)?;
    let params = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Nil => None,
            other => Some(niao_to_json(other, span)?),
        }
    } else {
        None
    };
    Ok(request_to_niao(&Request::notify(method, params)).ref_cell())
}

// >>> nrpc.success(1, "ok").result
fn nrpc_success(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrpc_success", span)?;
    let id = id_from_niao(&args[0].borrow(), span)?;
    let result = niao_to_json(&args[1].borrow(), span)?;
    Ok(response_to_niao(&Response::success(id, result)).ref_cell())
}

// >>> nrpc.failure(1, -32600, "bad").error.code
fn nrpc_failure(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nrpc_failure", span)?;
    let id = id_from_niao(&args[0].borrow(), span)?;
    let code = int_arg(args, 1, "nrpc_failure", span)?;
    let message = string_arg(args, 2, "nrpc_failure", span)?;
    let mut err = RpcError::new(code, message);
    if args.len() == 4 {
        err.data = Some(niao_to_json(&args[3].borrow(), span)?);
    }
    Ok(response_to_niao(&Response::error(id, err)).ref_cell())
}

// >>> nrpc.err(-32602, "bad").__nrpc_error
fn nrpc_err_builtin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nrpc_err", span)?;
    let code = int_arg(args, 0, "nrpc_err", span)?;
    let message = string_arg(args, 1, "nrpc_err", span)?;
    let data = if args.len() == 3 {
        Some(niao_to_json(&args[2].borrow(), span)?)
    } else {
        None
    };
    Ok(err_marker(code, message, data).ref_cell())
}

// >>> nrpc.ok(42)
fn nrpc_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_ok", span)?;
    Ok(Rc::clone(&args[0]))
}

macro_rules! std_err_fn {
    ($fn_name:ident, $code:expr, $doc:expr) => {
        fn $fn_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
            arity_range(args, 0, 1, stringify!($fn_name), span)?;
            let id = if args.is_empty() {
                Id::Null
            } else {
                id_from_niao(&args[0].borrow(), span)?
            };
            let err = RpcError::new($code, RpcError::default_message($code));
            Ok(response_to_niao(&Response::error(id, err)).ref_cell())
        }
    };
}

std_err_fn!(nrpc_parse_error, rpc_codes::PARSE_ERROR, "");
std_err_fn!(nrpc_invalid_request, rpc_codes::INVALID_REQUEST, "");
std_err_fn!(nrpc_method_not_found, rpc_codes::METHOD_NOT_FOUND, "");
std_err_fn!(nrpc_invalid_params, rpc_codes::INVALID_PARAMS, "");
std_err_fn!(nrpc_internal_error, rpc_codes::INTERNAL_ERROR, "");

// >>> nrpc.encode(nrpc.request("x", nil, 1)).len > 0
fn nrpc_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_encode", span)?;
    let j = niao_to_json(&args[0].borrow(), span)?;
    // Validate it is a JSON-RPC message when possible; still encode any object/array.
    let _ = parse_message_value(&j);
    let text = encode_value(&j);
    if text.len() > MAX_BYTES {
        return Ok(nrpc_err(span, format!("payload exceeds {MAX_BYTES} bytes")));
    }
    Ok(Value::String(text).ref_cell())
}

// >>> nrpc.decode("{\"jsonrpc\":\"2.0\",\"method\":\"x\",\"id\":1}").method
fn nrpc_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_decode", span)?;
    let text = text_arg(args, 0, "nrpc_decode", span)?;
    match decode(&text) {
        Ok(msg) => Ok(json_to_niao(msg.to_value()).ref_cell()),
        Err(e) => Ok(map_engine(span, e)),
    }
}

// >>> nrpc.encode_batch([nrpc.request("a", nil, 1), nrpc.request("b", nil, 2)]).starts_with("[")
fn nrpc_encode_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_encode_batch", span)?;
    match &*args[0].borrow() {
        Value::Array(items) => {
            if items.is_empty() {
                return Ok(nrpc_err(span, "batch must be non-empty"));
            }
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(niao_to_json(&it.borrow(), span)?);
            }
            Ok(Value::String(encode_value(&JsonValue::Array(out))).ref_cell())
        }
        other => Err(type_err(
            span,
            format!("encode_batch expects array, got {}", other.type_name()),
        )),
    }
}

// >>> nrpc.valid("{\"jsonrpc\":\"2.0\",\"method\":\"x\",\"id\":1}")
fn nrpc_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_valid", span)?;
    let text = text_arg(args, 0, "nrpc_valid", span)?;
    Ok(Value::Bool(valid(&text)).ref_cell())
}

fn msg_kind(v: &Value) -> &'static str {
    match v {
        Value::Array(_) => "batch",
        Value::Object(map) => {
            if map.contains_key("method") {
                if map.contains_key("id") {
                    "request"
                } else {
                    "notification"
                }
            } else if map.contains_key("error") {
                "error"
            } else if map.contains_key("result") {
                "response"
            } else {
                "unknown"
            }
        }
        _ => "unknown",
    }
}

// >>> nrpc.is_request(nrpc.request("x", nil, 1))
fn nrpc_is_request(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_is_request", span)?;
    Ok(Value::Bool(msg_kind(&args[0].borrow()) == "request").ref_cell())
}

// >>> nrpc.is_notification(nrpc.notify("x"))
fn nrpc_is_notification(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_is_notification", span)?;
    Ok(Value::Bool(msg_kind(&args[0].borrow()) == "notification").ref_cell())
}

// >>> nrpc.is_response(nrpc.success(1, true))
fn nrpc_is_response(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_is_response", span)?;
    let k = msg_kind(&args[0].borrow());
    Ok(Value::Bool(k == "response" || k == "error").ref_cell())
}

// >>> nrpc.is_error(nrpc.failure(1, -32600, "x"))
fn nrpc_is_error(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_is_error", span)?;
    Ok(Value::Bool(msg_kind(&args[0].borrow()) == "error").ref_cell())
}

// >>> nrpc.is_batch([nrpc.request("a", nil, 1)])
fn nrpc_is_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_is_batch", span)?;
    Ok(Value::Bool(msg_kind(&args[0].borrow()) == "batch").ref_cell())
}

// >>> type(nrpc.new_server())
fn nrpc_new_server(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrpc_new_server", span)?;
    let id = alloc_handle();
    SERVERS.with(|m| {
        m.borrow_mut().insert(
            id,
            ServerStore {
                methods: HashMap::new(),
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

// >>> let s = nrpc.new_server(); nrpc.method(s, "ping", "ping_handler"); nrpc.close(s); true
fn nrpc_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nrpc_method", span)?;
    let id = int_arg(args, 0, "nrpc_method", span)?;
    let name = string_arg(args, 1, "nrpc_method", span)?;
    if name.is_empty() {
        return Ok(nrpc_err(span, "method name must be non-empty"));
    }
    let handler = match &*args[2].borrow() {
        Value::Function(_) | Value::NativeFunction(_) => Handler::Callable(Rc::clone(&args[2])),
        Value::String(s) => {
            if s.is_empty() {
                return Ok(nrpc_err(span, "handler name must be non-empty"));
            }
            Handler::Named(s.clone())
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "method handler must be callable or function name string, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let ok = SERVERS.with(|m| {
        if let Some(s) = m.borrow_mut().get_mut(&id) {
            s.methods.insert(name, handler);
            true
        } else {
            false
        }
    });
    if ok {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(invalid_handle(span, "server", id))
    }
}

// >>> let s = nrpc.new_server(); nrpc.method(s, "a", fn(p){1}); let n = nrpc.methods(s); nrpc.close(s); n
fn nrpc_methods(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_methods", span)?;
    let id = int_arg(args, 0, "nrpc_methods", span)?;
    SERVERS.with(|m| {
        match m.borrow().get(&id) {
            Some(s) => {
                let mut names: Vec<_> = s.methods.keys().cloned().collect();
                names.sort();
                let arr = names
                    .into_iter()
                    .map(|n| Value::String(n).ref_cell())
                    .collect();
                Ok(Value::Array(arr).ref_cell())
            }
            None => Ok(invalid_handle(span, "server", id)),
        }
    })
}

// >>> let s = nrpc.new_server(); nrpc.method(s, "ping", fn(p) { "pong" }); let r = nrpc.dispatch(s, "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}"); nrpc.close(s); r.result
fn nrpc_dispatch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrpc_dispatch", span)?;
    let id = int_arg(args, 0, "nrpc_dispatch", span)?;
    let input = match &*args[1].borrow() {
        Value::String(s) => s.clone(),
        Value::ByteArray(b) => String::from_utf8(b.clone()).map_err(|_| {
            type_err(span, "dispatch byte[] must be valid UTF-8")
        })?,
        other => {
            let j = niao_to_json(other, span)?;
            encode_value(&j)
        }
    };
    server_dispatch_str(id, &input, span)
}

// >>> let s = nrpc.new_server(); nrpc.close(s)
fn nrpc_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_close", span)?;
    let id = int_arg(args, 0, "nrpc_close", span)?;
    let removed = SERVERS.with(|m| m.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(invalid_handle(span, "server", id))
    }
}

// >>> type(nrpc.new_client())
fn nrpc_new_client(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrpc_new_client", span)?;
    let id = alloc_handle();
    CLIENTS.with(|m| {
        m.borrow_mut().insert(id, ClientStore { next_id: 1 });
    });
    Ok(Value::Int(id).ref_cell())
}

// >>> let c = nrpc.new_client(); let r = nrpc.call(c, "sum", [1,2]); nrpc.close_client(c); r.id
fn nrpc_call(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nrpc_call", span)?;
    let id = int_arg(args, 0, "nrpc_call", span)?;
    let method = string_arg(args, 1, "nrpc_call", span)?;
    let params = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Nil => None,
            other => Some(niao_to_json(other, span)?),
        }
    } else {
        None
    };
    let req_id = CLIENTS.with(|m| {
        m.borrow_mut().get_mut(&id).map(|c| {
            let n = c.next_id;
            c.next_id += 1;
            n
        })
    });
    match req_id {
        Some(n) => Ok(request_to_niao(&Request::call(method, params, Id::Number(n))).ref_cell()),
        None => Ok(invalid_handle(span, "client", id)),
    }
}

// >>> let c = nrpc.new_client(); let n = nrpc.notify_call(c, "log", {x:1}); nrpc.close_client(c); n.method
fn nrpc_notify_call(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nrpc_notify_call", span)?;
    let id = int_arg(args, 0, "nrpc_notify_call", span)?;
    let exists = CLIENTS.with(|m| m.borrow().contains_key(&id));
    if !exists {
        return Ok(invalid_handle(span, "client", id));
    }
    let method = string_arg(args, 1, "nrpc_notify_call", span)?;
    let params = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Nil => None,
            other => Some(niao_to_json(other, span)?),
        }
    } else {
        None
    };
    Ok(request_to_niao(&Request::notify(method, params)).ref_cell())
}

// >>> let c = nrpc.new_client(); let n = nrpc.next_id(c); nrpc.close_client(c); n
fn nrpc_next_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_next_id", span)?;
    let id = int_arg(args, 0, "nrpc_next_id", span)?;
    CLIENTS.with(|m| match m.borrow().get(&id) {
        Some(c) => Ok(Value::Int(c.next_id).ref_cell()),
        None => Ok(invalid_handle(span, "client", id)),
    })
}

// >>> nrpc.parse_result(nrpc.success(1, 42)).ok
fn nrpc_parse_result(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_parse_result", span)?;
    let j = niao_to_json(&args[0].borrow(), span)?;
    // Accept either response object or encoded string already converted.
    let resp = match parse_response_value(&j) {
        Ok(r) => r,
        Err(e) => {
            // Maybe it's a raw string inside? or decode from string value
            if let JsonValue::String(s) = &j {
                match decode(s) {
                    Ok(Message::Response(r)) => r,
                    Ok(_) => return Ok(parse_err(span, "expected a JSON-RPC response")),
                    Err(e2) => return Ok(map_engine(span, e2)),
                }
            } else {
                return Ok(map_engine(span, e));
            }
        }
    };
    let mut out = HashMap::new();
    out.insert("id".into(), id_to_niao(&resp.id).ref_cell());
    match &resp.body {
        ResponseBody::Success(v) => {
            out.insert("ok".into(), Value::Bool(true).ref_cell());
            out.insert("result".into(), json_to_niao(v.clone()).ref_cell());
        }
        ResponseBody::Error(e) => {
            out.insert("ok".into(), Value::Bool(false).ref_cell());
            let mut err = HashMap::new();
            err.insert("code".into(), Value::Int(e.code).ref_cell());
            err.insert("message".into(), Value::String(e.message.clone()).ref_cell());
            if let Some(ref d) = e.data {
                err.insert("data".into(), json_to_niao(d.clone()).ref_cell());
            }
            out.insert("error".into(), Value::Object(err).ref_cell());
        }
    }
    Ok(Value::Object(out).ref_cell())
}

// >>> let c = nrpc.new_client(); nrpc.close_client(c)
fn nrpc_close_client(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nrpc_close_client", span)?;
    let id = int_arg(args, 0, "nrpc_close_client", span)?;
    let removed = CLIENTS.with(|m| m.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(invalid_handle(span, "client", id))
    }
}

// >>> nrpc.frame(nrpc.request("x", nil, 1)).ends_with("\n")
fn nrpc_frame(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nrpc_frame", span)?;
    let j = niao_to_json(&args[0].borrow(), span)?;
    let text = encode_value(&j);
    let style = style_from_arg(args, 1, "nrpc_frame", span)?;
    Ok(Value::String(frame_text(&text, style)).ref_cell())
}

// >>> nrpc.unframe(nrpc.frame(nrpc.request("x", nil, 1))).messages[0].method
fn nrpc_unframe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nrpc_unframe", span)?;
    let buf = text_arg(args, 0, "nrpc_unframe", span)?;
    let style = style_from_arg(args, 1, "nrpc_unframe", span)?;
    match unframe(&buf, style) {
        Ok(ur) => {
            let msgs: Vec<ValueRef> = ur
                .messages
                .into_iter()
                .map(|m| json_to_niao(m.to_value()).ref_cell())
                .collect();
            let mut out = HashMap::new();
            out.insert("messages".into(), Value::Array(msgs).ref_cell());
            out.insert("rest".into(), Value::String(ur.rest).ref_cell());
            Ok(Value::Object(out).ref_cell())
        }
        Err(e) => Ok(map_engine(span, e)),
    }
}

// >>> let s = nrpc.new_server(); nrpc.method(s, "ping", fn(p){"pong"}); let o = nrpc.stdio_exchange(s, "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}"); nrpc.close(s); o.contains("pong")
fn nrpc_stdio_exchange(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nrpc_stdio_exchange", span)?;
    let id = int_arg(args, 0, "nrpc_stdio_exchange", span)?;
    let input = text_arg(args, 1, "nrpc_stdio_exchange", span)?;
    let style = style_from_arg(args, 2, "nrpc_stdio_exchange", span)?;
    if !SERVERS.with(|m| m.borrow().contains_key(&id)) {
        return Ok(invalid_handle(span, "server", id));
    }
    let mut call = server_call_fn(id, span);
    let out = stdio_exchange(&input, style, &mut call);
    Ok(Value::String(out).ref_cell())
}

// >>> let s = nrpc.new_server(); nrpc.method(s, "ping", fn(p){"pong"}); let o = nrpc.handle_http_body(s, "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}"); nrpc.close(s); o.contains("pong")
fn nrpc_handle_http_body(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nrpc_handle_http_body", span)?;
    let id = int_arg(args, 0, "nrpc_handle_http_body", span)?;
    let body = text_arg(args, 1, "nrpc_handle_http_body", span)?;
    if !SERVERS.with(|m| m.borrow().contains_key(&id)) {
        return Ok(invalid_handle(span, "server", id));
    }
    let mut call = server_call_fn(id, span);
    let out = handle_payload(&body, &mut call);
    Ok(Value::String(out).ref_cell())
}

// TCP serve once — blocks until one client connects.
// >>> true
fn nrpc_tcp_serve_once(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nrpc_tcp_serve_once", span)?;
    let id = int_arg(args, 0, "nrpc_tcp_serve_once", span)?;
    let host = string_arg(args, 1, "nrpc_tcp_serve_once", span)?;
    let port = int_arg(args, 2, "nrpc_tcp_serve_once", span)?;
    if !(0..=65535).contains(&port) {
        return Ok(nrpc_err(span, "port out of range"));
    }
    let opts = transport_opts_from(args, 3, span)?;
    if !SERVERS.with(|m| m.borrow().contains_key(&id)) {
        return Ok(invalid_handle(span, "server", id));
    }
    let mut call = server_call_fn(id, span);
    match tcp_serve_once((host.as_str(), port as u16), &opts, &mut call) {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(map_engine(span, e)),
    }
}

// >>> true
fn nrpc_tcp_call(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 5, "nrpc_tcp_call", span)?;
    let host = string_arg(args, 0, "nrpc_tcp_call", span)?;
    let port = int_arg(args, 1, "nrpc_tcp_call", span)?;
    if !(0..=65535).contains(&port) {
        return Ok(nrpc_err(span, "port out of range"));
    }
    let method = string_arg(args, 2, "nrpc_tcp_call", span)?;
    let params = if args.len() >= 4 {
        match &*args[3].borrow() {
            Value::Nil => None,
            other => Some(niao_to_json(other, span)?),
        }
    } else {
        None
    };
    let opts = transport_opts_from(args, 4, span)?;
    match tcp_call(
        (host.as_str(), port as u16),
        &method,
        params,
        Id::Number(1),
        &opts,
    ) {
        Ok(r) => Ok(response_to_niao(&r).ref_cell()),
        Err(e) => Ok(map_engine(span, e)),
    }
}

// >>> true
fn nrpc_http_serve_once(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nrpc_http_serve_once", span)?;
    let id = int_arg(args, 0, "nrpc_http_serve_once", span)?;
    let host = string_arg(args, 1, "nrpc_http_serve_once", span)?;
    let port = int_arg(args, 2, "nrpc_http_serve_once", span)?;
    if !(0..=65535).contains(&port) {
        return Ok(nrpc_err(span, "port out of range"));
    }
    let path = if args.len() == 4 {
        string_arg(args, 3, "nrpc_http_serve_once", span)?
    } else {
        "/".into()
    };
    if !SERVERS.with(|m| m.borrow().contains_key(&id)) {
        return Ok(invalid_handle(span, "server", id));
    }
    let mut call = server_call_fn(id, span);
    match http_serve_once((host.as_str(), port as u16), &path, &mut call) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_engine(span, e)),
    }
}

// >>> true
fn nrpc_http_call(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nrpc_http_call", span)?;
    let url = string_arg(args, 0, "nrpc_http_call", span)?;
    let method = string_arg(args, 1, "nrpc_http_call", span)?;
    let params = if args.len() == 3 {
        match &*args[2].borrow() {
            Value::Nil => None,
            other => Some(niao_to_json(other, span)?),
        }
    } else {
        None
    };
    match http_call(&url, &method, params, Id::Number(1)) {
        Ok(r) => Ok(response_to_niao(&r).ref_cell()),
        Err(e) => Ok(map_engine(span, e)),
    }
}

// Error code constants as functions for discoverability.
// >>> nrpc.PARSE_ERROR()
fn nrpc_parse_error_code(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrpc_PARSE_ERROR", span)?;
    Ok(Value::Int(rpc_codes::PARSE_ERROR).ref_cell())
}
fn nrpc_invalid_request_code(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrpc_INVALID_REQUEST", span)?;
    Ok(Value::Int(rpc_codes::INVALID_REQUEST).ref_cell())
}
fn nrpc_method_not_found_code(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrpc_METHOD_NOT_FOUND", span)?;
    Ok(Value::Int(rpc_codes::METHOD_NOT_FOUND).ref_cell())
}
fn nrpc_invalid_params_code(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrpc_INVALID_PARAMS", span)?;
    Ok(Value::Int(rpc_codes::INVALID_PARAMS).ref_cell())
}
fn nrpc_internal_error_code(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nrpc_INTERNAL_ERROR", span)?;
    Ok(Value::Int(rpc_codes::INTERNAL_ERROR).ref_cell())
}

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("nrpc_request", "request", Rc::new(nrpc_request) as NativeFn),
        ("nrpc_notify", "notify", Rc::new(nrpc_notify) as NativeFn),
        ("nrpc_success", "success", Rc::new(nrpc_success) as NativeFn),
        ("nrpc_failure", "failure", Rc::new(nrpc_failure) as NativeFn),
        ("nrpc_err", "err", Rc::new(nrpc_err_builtin) as NativeFn),
        ("nrpc_ok", "ok", Rc::new(nrpc_ok) as NativeFn),
        ("nrpc_parse_error", "parse_error", Rc::new(nrpc_parse_error) as NativeFn),
        ("nrpc_invalid_request", "invalid_request", Rc::new(nrpc_invalid_request) as NativeFn),
        ("nrpc_method_not_found", "method_not_found", Rc::new(nrpc_method_not_found) as NativeFn),
        ("nrpc_invalid_params", "invalid_params", Rc::new(nrpc_invalid_params) as NativeFn),
        ("nrpc_internal_error", "internal_error", Rc::new(nrpc_internal_error) as NativeFn),
        ("nrpc_encode", "encode", Rc::new(nrpc_encode) as NativeFn),
        ("nrpc_decode", "decode", Rc::new(nrpc_decode) as NativeFn),
        ("nrpc_encode_batch", "encode_batch", Rc::new(nrpc_encode_batch) as NativeFn),
        ("nrpc_valid", "valid", Rc::new(nrpc_valid) as NativeFn),
        ("nrpc_is_request", "is_request", Rc::new(nrpc_is_request) as NativeFn),
        ("nrpc_is_notification", "is_notification", Rc::new(nrpc_is_notification) as NativeFn),
        ("nrpc_is_response", "is_response", Rc::new(nrpc_is_response) as NativeFn),
        ("nrpc_is_error", "is_error", Rc::new(nrpc_is_error) as NativeFn),
        ("nrpc_is_batch", "is_batch", Rc::new(nrpc_is_batch) as NativeFn),
        ("nrpc_new_server", "new_server", Rc::new(nrpc_new_server) as NativeFn),
        ("nrpc_method", "method", Rc::new(nrpc_method) as NativeFn),
        ("nrpc_methods", "methods", Rc::new(nrpc_methods) as NativeFn),
        ("nrpc_dispatch", "dispatch", Rc::new(nrpc_dispatch) as NativeFn),
        ("nrpc_close", "close", Rc::new(nrpc_close) as NativeFn),
        ("nrpc_new_client", "new_client", Rc::new(nrpc_new_client) as NativeFn),
        ("nrpc_call", "call", Rc::new(nrpc_call) as NativeFn),
        ("nrpc_notify_call", "notify_call", Rc::new(nrpc_notify_call) as NativeFn),
        ("nrpc_next_id", "next_id", Rc::new(nrpc_next_id) as NativeFn),
        ("nrpc_parse_result", "parse_result", Rc::new(nrpc_parse_result) as NativeFn),
        ("nrpc_close_client", "close_client", Rc::new(nrpc_close_client) as NativeFn),
        ("nrpc_frame", "frame", Rc::new(nrpc_frame) as NativeFn),
        ("nrpc_unframe", "unframe", Rc::new(nrpc_unframe) as NativeFn),
        ("nrpc_stdio_exchange", "stdio_exchange", Rc::new(nrpc_stdio_exchange) as NativeFn),
        ("nrpc_handle_http_body", "handle_http_body", Rc::new(nrpc_handle_http_body) as NativeFn),
        ("nrpc_tcp_serve_once", "tcp_serve_once", Rc::new(nrpc_tcp_serve_once) as NativeFn),
        ("nrpc_tcp_call", "tcp_call", Rc::new(nrpc_tcp_call) as NativeFn),
        ("nrpc_http_serve_once", "http_serve_once", Rc::new(nrpc_http_serve_once) as NativeFn),
        ("nrpc_http_call", "http_call", Rc::new(nrpc_http_call) as NativeFn),
        ("nrpc_PARSE_ERROR", "PARSE_ERROR", Rc::new(nrpc_parse_error_code) as NativeFn),
        ("nrpc_INVALID_REQUEST", "INVALID_REQUEST", Rc::new(nrpc_invalid_request_code) as NativeFn),
        ("nrpc_METHOD_NOT_FOUND", "METHOD_NOT_FOUND", Rc::new(nrpc_method_not_found_code) as NativeFn),
        ("nrpc_INVALID_PARAMS", "INVALID_PARAMS", Rc::new(nrpc_invalid_params_code) as NativeFn),
        ("nrpc_INTERNAL_ERROR", "INTERNAL_ERROR", Rc::new(nrpc_internal_error_code) as NativeFn),
    ]
}

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(full, _, f)| (full, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nrpc";
pub const MODULE_PATHS: &[&str] = &["nrpc", "std/nrpc"];

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
    fn request_roundtrip() {
        let args = vec![
            Value::String("ping".into()).ref_cell(),
            Value::Nil.ref_cell(),
            Value::Int(1).ref_cell(),
        ];
        let out = nrpc_request(&args, span()).unwrap();
        match &*out.borrow() {
            Value::Object(m) => {
                assert_eq!(m.get("method").map(|v| &*v.borrow()), Some(&Value::String("ping".into())));
                assert_eq!(m.get("id").map(|v| &*v.borrow()), Some(&Value::Int(1)));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_ping() {
        fn ping_handler(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
            Ok(Value::String("pong".into()).ref_cell())
        }

        let srv = nrpc_new_server(&[], span()).unwrap();
        let id = match &*srv.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int handle, got {other:?}"),
        };
        let handler = Value::NativeFunction(Rc::new(ping_handler)).ref_cell();
        let reg = vec![
            Value::Int(id).ref_cell(),
            Value::String("ping".into()).ref_cell(),
            handler,
        ];
        assert!(matches!(&*nrpc_method(&reg, span()).unwrap().borrow(), Value::Bool(true)));

        let dispatch_args = vec![
            Value::Int(id).ref_cell(),
            Value::String(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#.into()).ref_cell(),
        ];
        let resp = nrpc_dispatch(&dispatch_args, span()).unwrap();
        match &*resp.borrow() {
            Value::Object(m) => {
                let result = m.get("result").map(|v| &*v.borrow());
                assert_eq!(result, Some(&Value::String("pong".into())));
            }
            other => panic!("expected response object, got {other:?}"),
        }
        let close = vec![Value::Int(id).ref_cell()];
        assert!(matches!(&*nrpc_close(&close, span()).unwrap().borrow(), Value::Bool(true)));
    }

    #[test]
    fn encode_decode_valid() {
        let req_args = vec![
            Value::String("x".into()).ref_cell(),
            Value::Nil.ref_cell(),
            Value::Int(1).ref_cell(),
        ];
        let req = nrpc_request(&req_args, span()).unwrap();
        let enc = nrpc_encode(&[req], span()).unwrap();
        let text = match &*enc.borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        };
        assert!(nrpc_valid(&[Value::String(text.clone()).ref_cell()], span())
            .map(|v| matches!(&*v.borrow(), Value::Bool(true)))
            .unwrap());
        let dec = nrpc_decode(&[Value::String(text).ref_cell()], span()).unwrap();
        match &*dec.borrow() {
            Value::Object(m) => assert_eq!(
                m.get("method").map(|v| &*v.borrow()),
                Some(&Value::String("x".into()))
            ),
            other => panic!("expected object, got {other:?}"),
        }
    }
}
