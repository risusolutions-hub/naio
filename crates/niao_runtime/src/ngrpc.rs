//! Native ngrpc standard library — gRPC client/server over HTTP/2 (h2c),
//! framing, streaming RPCs (~grpcio subset). Message bytes use `nproto`.
//!
//! Import with `import "ngrpc"` (or `import "std/ngrpc"`).

use crate::{
    call_niao_function, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef,
};
use niao_ast::Span;
use niao_errors::codes;
use niao_grpc::{
    frame_message, method_path as grpc_method_path, normalize_method_path, parse_method,
    unframe_all, unframe_one, CallOptions, Channel, ClientCall, GrpcError, GrpcServer,
    HandlerReply, IncomingRpc, MethodKind, RpcResult, Status, StatusCode, SyncHandler,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const E_ARITY: u32 = codes::E4110_NGRPC_ARITY;
const E_ERROR: u32 = codes::E4111_NGRPC_ERROR;
const E_TYPE: u32 = codes::E4112_NGRPC_TYPE;
const E_PROTOCOL: u32 = codes::E4113_NGRPC_PROTOCOL;
const E_HANDLE: u32 = codes::E4114_NGRPC_INVALID_HANDLE;
const KIND: &str = "ngrpc_error";

// ---------------------------------------------------------------------------
// Handle stores
// ---------------------------------------------------------------------------

struct ServerState {
    grpc: GrpcServer,
    niao_routes: HashMap<String, (MethodKind, ValueRef)>,
    has_sync: bool,
}

thread_local! {
    static CHANNELS: RefCell<HashMap<i64, Channel>> = RefCell::new(HashMap::new());
    static CALLS: RefCell<HashMap<i64, ClientCall>> = RefCell::new(HashMap::new());
    static SERVERS: RefCell<HashMap<i64, ServerState>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_handle() -> i64 {
    NEXT_HANDLE.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n = n.saturating_add(1);
        id
    })
}

fn alloc_channel(ch: Channel) -> i64 {
    let id = alloc_handle();
    CHANNELS.with(|m| m.borrow_mut().insert(id, ch));
    id
}

fn alloc_call(call: ClientCall) -> i64 {
    let id = alloc_handle();
    CALLS.with(|m| m.borrow_mut().insert(id, call));
    id
}

fn alloc_server(state: ServerState) -> i64 {
    let id = alloc_handle();
    SERVERS.with(|m| m.borrow_mut().insert(id, state));
    id
}

// ---------------------------------------------------------------------------
// Errors / arity
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn ngrpc_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E_ERROR, KIND, msg.into(), span)
}

fn protocol_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E_PROTOCOL, KIND, msg.into(), span)
}

fn invalid_handle(span: Span, kind: &str, id: i64) -> ValueRef {
    error_value(
        E_HANDLE,
        KIND,
        format!("invalid or closed {kind} handle {id}"),
        span,
    )
}

fn map_grpc(span: Span, err: GrpcError) -> ValueRef {
    let msg = err.to_string();
    let lower = msg.to_ascii_lowercase();
    if lower.contains("frame")
        || lower.contains("truncated")
        || lower.contains("compressed")
        || lower.contains("message length")
        || lower.contains("message too large")
        || lower.contains("method path")
        || lower.contains("method kind")
        || lower.contains("grpc-status")
    {
        protocol_err(span, msg)
    } else {
        ngrpc_err(span, msg)
    }
}

// ---------------------------------------------------------------------------
// Arg helpers
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

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] or string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn function_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    match &*args[idx].borrow() {
        Value::Function(_) | Value::NativeFunction(_) => Ok(Rc::clone(&args[idx])),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a function as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn metadata_to_value(meta: &HashMap<String, String>) -> ValueRef {
    let mut map = HashMap::with_capacity(meta.len());
    for (k, v) in meta {
        map.insert(k.clone(), Value::String(v.clone()).ref_cell());
    }
    Value::Object(map).ref_cell()
}

fn value_to_metadata(v: &Value, name: &str, span: Span) -> NiaoResult<HashMap<String, String>> {
    match v {
        Value::Object(map) => {
            let mut out = HashMap::new();
            for (k, val) in map {
                match &*val.borrow() {
                    Value::String(s) => {
                        out.insert(k.trim().to_ascii_lowercase(), s.clone());
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() headers values must be strings, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() headers must be an object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn call_options_from_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<CallOptions> {
    if idx >= args.len() {
        return Ok(CallOptions::default());
    }
    match &*args[idx].borrow() {
        Value::Nil => Ok(CallOptions::default()),
        Value::Object(map) => {
            let mut opts = CallOptions::default();
            if let Some(h) = map.get("headers") {
                opts.headers = value_to_metadata(&h.borrow(), name, span)?;
            }
            if let Some(t) = map.get("timeout_ms") {
                match &*t.borrow() {
                    Value::Int(n) if *n >= 0 => {
                        opts.timeout = Some(Duration::from_millis(*n as u64));
                    }
                    Value::Nil => {}
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() timeout_ms must be a non-negative int, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            if let Some(a) = map.get("authority") {
                match &*a.borrow() {
                    Value::String(s) => opts.authority = Some(s.clone()),
                    Value::Nil => {}
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() authority must be a string, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(opts)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects opts object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_val(bytes: Vec<u8>) -> ValueRef {
    Value::ByteArray(bytes).ref_cell()
}

fn messages_to_value(messages: &[Vec<u8>]) -> ValueRef {
    Value::Array(messages.iter().cloned().map(bytes_val).collect()).ref_cell()
}

fn rpc_result_value(r: &RpcResult) -> ValueRef {
    let mut map = HashMap::new();
    map.insert(
        "status".into(),
        Value::Int(r.status.code.as_i32() as i64).ref_cell(),
    );
    map.insert(
        "message".into(),
        Value::String(r.status.message.clone()).ref_cell(),
    );
    map.insert("bytes".into(), bytes_val(r.bytes.clone()));
    map.insert("messages".into(), messages_to_value(&r.messages));
    map.insert("headers".into(), metadata_to_value(&r.headers));
    map.insert("trailers".into(), metadata_to_value(&r.trailers));
    Value::Object(map).ref_cell()
}

fn handler_reply_to_rpc(reply: HandlerReply) -> RpcResult {
    let bytes = reply.messages.first().cloned().unwrap_or_default();
    RpcResult {
        status: reply.status,
        bytes,
        messages: reply.messages,
        headers: reply.headers,
        trailers: HashMap::new(),
    }
}

fn parse_handler_reply(v: &ValueRef, span: Span) -> HandlerReply {
    match &*v.borrow() {
        Value::ByteArray(b) => HandlerReply::ok_bytes(b.clone()),
        Value::String(s) => HandlerReply::ok_bytes(s.as_bytes().to_vec()),
        Value::Array(items) => {
            let mut messages = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::ByteArray(b) => messages.push(b.clone()),
                    Value::String(s) => messages.push(s.as_bytes().to_vec()),
                    _ => {}
                }
            }
            HandlerReply::ok_messages(messages)
        }
        Value::Object(map) => {
            let mut messages = Vec::new();
            if let Some(b) = map.get("bytes") {
                match &*b.borrow() {
                    Value::ByteArray(bytes) => messages.push(bytes.clone()),
                    Value::String(s) => messages.push(s.as_bytes().to_vec()),
                    _ => {}
                }
            }
            if let Some(arr) = map.get("messages") {
                if let Value::Array(items) = &*arr.borrow() {
                    for item in items {
                        match &*item.borrow() {
                            Value::ByteArray(b) => messages.push(b.clone()),
                            Value::String(s) => messages.push(s.as_bytes().to_vec()),
                            _ => {}
                        }
                    }
                }
            }
            let code = map
                .get("status")
                .and_then(|s| match &*s.borrow() {
                    Value::Int(n) => StatusCode::from_i32(*n as i32),
                    Value::String(name) => StatusCode::from_name(name),
                    _ => None,
                })
                .unwrap_or(StatusCode::Ok);
            let message = map
                .get("message")
                .and_then(|m| match &*m.borrow() {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let headers = map
                .get("headers")
                .map(|h| value_to_metadata(&h.borrow(), "ngrpc_handler", span).unwrap_or_default())
                .unwrap_or_default();
            HandlerReply {
                status: Status::new(code, message),
                messages,
                headers,
            }
        }
        Value::Error(e) => HandlerReply::status_only(Status::new(
            StatusCode::Internal,
            e.message.clone(),
        )),
        Value::Nil => HandlerReply::ok_bytes(Vec::new()),
        _ => HandlerReply::status_only(Status::new(
            StatusCode::Internal,
            "handler returned unsupported value",
        )),
    }
}

fn messages_from_value(v: &Value, name: &str, span: Span) -> NiaoResult<Vec<Vec<u8>>> {
    match v {
        Value::Nil => Ok(Vec::new()),
        Value::ByteArray(b) => Ok(vec![b.clone()]),
        Value::String(s) => Ok(vec![s.as_bytes().to_vec()]),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::ByteArray(b) => out.push(b.clone()),
                    Value::String(s) => out.push(s.as_bytes().to_vec()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() messages[{}] must be byte[] or string, got {}",
                                i,
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
                "{name}() expects byte[], string, or array of messages, got {}",
                other.type_name()
            ),
        )),
    }
}

fn handler_payload_args(rpc: &IncomingRpc) -> Vec<ValueRef> {
    let meta = metadata_to_value(&rpc.metadata);
    let payload = match rpc.kind {
        MethodKind::Unary | MethodKind::ServerStream => {
            let first = rpc.messages.first().cloned().unwrap_or_default();
            bytes_val(first)
        }
        MethodKind::ClientStream | MethodKind::Bidi => messages_to_value(&rpc.messages),
    };
    vec![payload, meta]
}

fn call_niao_route(
    handler: &ValueRef,
    kind: MethodKind,
    mut rpc: IncomingRpc,
    span: Span,
) -> HandlerReply {
    rpc.kind = kind;
    let args = handler_payload_args(&rpc);
    match call_niao_function(Rc::clone(handler), &args, span) {
        Ok(v) => parse_handler_reply(&v, span),
        Err(e) => HandlerReply::status_only(Status::new(StatusCode::Internal, e.message())),
    }
}

fn dispatch_from_routes(
    routes: &HashMap<String, (MethodKind, ValueRef)>,
    rpc: IncomingRpc,
    span: Span,
) -> HandlerReply {
    match routes.get(&rpc.method) {
        Some((kind, handler)) => call_niao_route(handler, *kind, rpc, span),
        None => HandlerReply::status_only(Status::new(
            StatusCode::Unimplemented,
            format!("unknown method {}", rpc.method),
        )),
    }
}

/// Take a server out of the store so `poll_with` / `serve_with` can re-enter
/// `call_niao_function` without a `RefCell` borrow conflict.
fn with_server_taken<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut ServerState) -> T,
) -> Result<T, ValueRef> {
    let state = SERVERS.with(|m| m.borrow_mut().remove(&id));
    match state {
        None => Err(invalid_handle(span, "server", id)),
        Some(mut s) => {
            let out = f(&mut s);
            SERVERS.with(|m| m.borrow_mut().insert(id, s));
            Ok(out)
        }
    }
}

fn with_channel_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Channel) -> Result<T, GrpcError>,
) -> NiaoResult<Result<T, ValueRef>> {
    CHANNELS.with(|m| match m.borrow().get(&id) {
        Some(ch) => match f(ch) {
            Ok(v) => Ok(Ok(v)),
            Err(e) => Ok(Err(map_grpc(span, e))),
        },
        None => Ok(Err(invalid_handle(span, "channel", id))),
    })
}

fn with_call_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut ClientCall) -> Result<T, GrpcError>,
) -> NiaoResult<Result<T, ValueRef>> {
    CALLS.with(|m| match m.borrow_mut().get_mut(&id) {
        Some(call) => match f(call) {
            Ok(v) => Ok(Ok(v)),
            Err(e) => Ok(Err(map_grpc(span, e))),
        },
        None => Ok(Err(invalid_handle(span, "call", id))),
    })
}

// ---------------------------------------------------------------------------
// Builtins — status / method / framing
// ---------------------------------------------------------------------------

// >>> ngrpc.status_ok()
// => 0
fn ngrpc_status_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ngrpc_status_ok", span)?;
    Ok(Value::Int(StatusCode::Ok.as_i32() as i64).ref_cell())
}

// >>> ngrpc.status_name(0)
// => "OK"
fn ngrpc_status_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_status_name", span)?;
    let code = int_arg(args, 0, "ngrpc_status_name", span)?;
    match StatusCode::from_i32(code as i32) {
        Some(sc) => Ok(Value::String(sc.name().to_string()).ref_cell()),
        None => Ok(protocol_err(span, format!("unknown status code {code}"))),
    }
}

// >>> ngrpc.status_code("OK")
// => 0
fn ngrpc_status_code(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_status_code", span)?;
    let name = string_arg(args, 0, "ngrpc_status_code", span)?;
    match StatusCode::from_name(&name) {
        Some(sc) => Ok(Value::Int(sc.as_i32() as i64).ref_cell()),
        None => Ok(protocol_err(span, format!("unknown status name '{name}'"))),
    }
}

// >>> ngrpc.method_path("echo.Echo", "Say")
// => "/echo.Echo/Say"
fn ngrpc_method_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngrpc_method_path", span)?;
    let service = string_arg(args, 0, "ngrpc_method_path", span)?;
    let method = string_arg(args, 1, "ngrpc_method_path", span)?;
    match grpc_method_path(&service, &method) {
        Ok(path) => Ok(Value::String(path).ref_cell()),
        Err(e) => Ok(map_grpc(span, e)),
    }
}

// >>> ngrpc.parse_method("/echo.Echo/Say").service
// => "echo.Echo"
fn ngrpc_parse_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_parse_method", span)?;
    let path = string_arg(args, 0, "ngrpc_parse_method", span)?;
    match parse_method(&path) {
        Ok((service, method)) => {
            let mut map = HashMap::new();
            map.insert("service".into(), Value::String(service).ref_cell());
            map.insert("method".into(), Value::String(method).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_grpc(span, e)),
    }
}

// >>> ngrpc.frame("hi").len
// => 7
fn ngrpc_frame(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_frame", span)?;
    let payload = bytes_arg(args, 0, "ngrpc_frame", span)?;
    match frame_message(&payload) {
        Ok(framed) => Ok(bytes_val(framed.to_vec())),
        Err(e) => Ok(map_grpc(span, e)),
    }
}

// >>> ngrpc.unframe(ngrpc.frame("hi"))
// => byte[]
fn ngrpc_unframe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_unframe", span)?;
    let data = bytes_arg(args, 0, "ngrpc_unframe", span)?;
    match unframe_one(&data) {
        Ok((payload, _)) => Ok(bytes_val(payload)),
        Err(e) => Ok(map_grpc(span, e)),
    }
}

// >>> ngrpc.unframe_all(ngrpc.frame("a")).len
// => 1
fn ngrpc_unframe_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_unframe_all", span)?;
    let data = bytes_arg(args, 0, "ngrpc_unframe_all", span)?;
    match unframe_all(&data) {
        Ok(msgs) => Ok(messages_to_value(&msgs)),
        Err(e) => Ok(map_grpc(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Client channel / calls
// ---------------------------------------------------------------------------

// >>> type(ngrpc.channel("127.0.0.1:50051"))
// => "int"
fn ngrpc_channel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngrpc_channel", span)?;
    let target = string_arg(args, 0, "ngrpc_channel", span)?;
    let opts = call_options_from_arg(args, 1, "ngrpc_channel", span)?;
    match Channel::connect(&target, &opts) {
        Ok(ch) => Ok(Value::Int(alloc_channel(ch)).ref_cell()),
        Err(e) => Ok(map_grpc(span, e)),
    }
}

// >>> let c = ngrpc.channel("127.0.0.1:1"); ngrpc.close_channel(c)
// => true
fn ngrpc_close_channel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_close_channel", span)?;
    let id = int_arg(args, 0, "ngrpc_close_channel", span)?;
    let removed = CHANNELS.with(|m| m.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(invalid_handle(span, "channel", id))
    }
}

// >>> let c = ngrpc.channel("127.0.0.1:1"); let t = ngrpc.channel_target(c); ngrpc.close_channel(c); t
// => "127.0.0.1:1"
fn ngrpc_channel_target(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_channel_target", span)?;
    let id = int_arg(args, 0, "ngrpc_channel_target", span)?;
    CHANNELS.with(|m| match m.borrow().get(&id) {
        Some(ch) => Ok(Value::String(ch.target().to_string()).ref_cell()),
        None => Ok(invalid_handle(span, "channel", id)),
    })
}

// >>> let r = ngrpc.unary(ch, "/svc/M", "ping"); r.status
// => 0
fn ngrpc_unary(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ngrpc_unary", span)?;
    let id = int_arg(args, 0, "ngrpc_unary", span)?;
    let method = string_arg(args, 1, "ngrpc_unary", span)?;
    let request = bytes_arg(args, 2, "ngrpc_unary", span)?;
    let opts = call_options_from_arg(args, 3, "ngrpc_unary", span)?;
    match with_channel_mut(id, span, |ch| ch.unary(&method, &request, &opts))? {
        Ok(r) => Ok(rpc_result_value(&r)),
        Err(e) => Ok(e),
    }
}

// >>> type(ngrpc.open_server_stream(ch, "/svc/S", "x"))
// => "int"
fn ngrpc_open_server_stream(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ngrpc_open_server_stream", span)?;
    let id = int_arg(args, 0, "ngrpc_open_server_stream", span)?;
    let method = string_arg(args, 1, "ngrpc_open_server_stream", span)?;
    let request = bytes_arg(args, 2, "ngrpc_open_server_stream", span)?;
    let opts = call_options_from_arg(args, 3, "ngrpc_open_server_stream", span)?;
    match with_channel_mut(id, span, |ch| ch.open_server_stream(&method, &request, &opts))? {
        Ok(call) => Ok(Value::Int(alloc_call(call)).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> type(ngrpc.open_client_stream(ch, "/svc/C"))
// => "int"
fn ngrpc_open_client_stream(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ngrpc_open_client_stream", span)?;
    let id = int_arg(args, 0, "ngrpc_open_client_stream", span)?;
    let method = string_arg(args, 1, "ngrpc_open_client_stream", span)?;
    let opts = call_options_from_arg(args, 2, "ngrpc_open_client_stream", span)?;
    match with_channel_mut(id, span, |ch| ch.open_client_stream(&method, &opts))? {
        Ok(call) => Ok(Value::Int(alloc_call(call)).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> type(ngrpc.open_bidi(ch, "/svc/B"))
// => "int"
fn ngrpc_open_bidi(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ngrpc_open_bidi", span)?;
    let id = int_arg(args, 0, "ngrpc_open_bidi", span)?;
    let method = string_arg(args, 1, "ngrpc_open_bidi", span)?;
    let opts = call_options_from_arg(args, 2, "ngrpc_open_bidi", span)?;
    match with_channel_mut(id, span, |ch| ch.open_bidi(&method, &opts))? {
        Ok(call) => Ok(Value::Int(alloc_call(call)).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> ngrpc.send(call, "chunk")
// => true
fn ngrpc_send(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngrpc_send", span)?;
    let id = int_arg(args, 0, "ngrpc_send", span)?;
    let payload = bytes_arg(args, 1, "ngrpc_send", span)?;
    match with_call_mut(id, span, |call| call.send(&payload))? {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> ngrpc.send_close(call)
// => true
fn ngrpc_send_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_send_close", span)?;
    let id = int_arg(args, 0, "ngrpc_send_close", span)?;
    match with_call_mut(id, span, |call| call.send_close())? {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> ngrpc.recv(call)
fn ngrpc_recv(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_recv", span)?;
    let id = int_arg(args, 0, "ngrpc_recv", span)?;
    match with_call_mut(id, span, |call| call.recv())? {
        Ok(Some(msg)) => Ok(bytes_val(msg)),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// >>> ngrpc.finish(call).status
// => 0
fn ngrpc_finish(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_finish", span)?;
    let id = int_arg(args, 0, "ngrpc_finish", span)?;
    match with_call_mut(id, span, |call| call.finish())? {
        Ok(r) => Ok(rpc_result_value(&r)),
        Err(e) => Ok(e),
    }
}

// >>> ngrpc.close_call(call)
// => true
fn ngrpc_close_call(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_close_call", span)?;
    let id = int_arg(args, 0, "ngrpc_close_call", span)?;
    let removed = CALLS.with(|m| m.borrow_mut().remove(&id).is_some());
    if removed {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(invalid_handle(span, "call", id))
    }
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

// >>> type(ngrpc.server("127.0.0.1:0"))
// => "int"
fn ngrpc_server(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ngrpc_server", span)?;
    let addr = if args.is_empty() {
        "127.0.0.1:0".to_string()
    } else {
        string_arg(args, 0, "ngrpc_server", span)?
    };
    match GrpcServer::bind(&addr) {
        Ok(grpc) => Ok(Value::Int(alloc_server(ServerState {
            grpc,
            niao_routes: HashMap::new(),
            has_sync: false,
        }))
        .ref_cell()),
        Err(e) => Ok(map_grpc(span, e)),
    }
}

// >>> let s = ngrpc.server(); let a = ngrpc.addr(s); ngrpc.close_server(s); a.contains(":")
// => true
fn ngrpc_addr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_addr", span)?;
    let id = int_arg(args, 0, "ngrpc_addr", span)?;
    SERVERS.with(|m| match m.borrow().get(&id) {
        Some(s) => Ok(Value::String(s.grpc.addr()).ref_cell()),
        None => Ok(invalid_handle(span, "server", id)),
    })
}

// >>> let s = ngrpc.server(); ngrpc.on(s, "/echo.Echo/Say", "unary", fn(req, md) { req }); ngrpc.close_server(s); true
// => true
fn ngrpc_on(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "ngrpc_on", span)?;
    let id = int_arg(args, 0, "ngrpc_on", span)?;
    let method = string_arg(args, 1, "ngrpc_on", span)?;
    let kind_s = string_arg(args, 2, "ngrpc_on", span)?;
    let handler = function_arg(args, 3, "ngrpc_on", span)?;
    let kind = match MethodKind::parse(&kind_s) {
        Ok(k) => k,
        Err(e) => return Ok(map_grpc(span, e)),
    };
    let path = match normalize_method_path(&method) {
        Ok(p) => p,
        Err(e) => return Ok(map_grpc(span, e)),
    };
    SERVERS.with(|m| {
        let mut map = m.borrow_mut();
        match map.get_mut(&id) {
            Some(s) => {
                s.niao_routes.insert(path, (kind, handler));
                Ok(Value::Bool(true).ref_cell())
            }
            None => Ok(invalid_handle(span, "server", id)),
        }
    })
}

// >>> let s = ngrpc.server(); ngrpc.mount_echo(s, "/echo.Echo/Echo"); ngrpc.close_server(s); true
// => true
fn ngrpc_mount_echo(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ngrpc_mount_echo", span)?;
    let id = int_arg(args, 0, "ngrpc_mount_echo", span)?;
    let method = string_arg(args, 1, "ngrpc_mount_echo", span)?;
    let handler: SyncHandler = Arc::new(|rpc: IncomingRpc| {
        let msg = rpc.messages.first().cloned().unwrap_or_default();
        HandlerReply::ok_bytes(msg)
    });
    SERVERS.with(|m| {
        let mut map = m.borrow_mut();
        match map.get_mut(&id) {
            Some(s) => match s.grpc.register(&method, MethodKind::Unary, handler) {
                Ok(()) => {
                    s.has_sync = true;
                    Ok(Value::Bool(true).ref_cell())
                }
                Err(e) => Ok(map_grpc(span, e)),
            },
            None => Ok(invalid_handle(span, "server", id)),
        }
    })
}

// >>> let s = ngrpc.server(); ngrpc.mount_echo(s, "/e/E"); ngrpc.serve_bg(s); ngrpc.stop(s); ngrpc.close_server(s); true
// => true
fn ngrpc_serve_bg(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_serve_bg", span)?;
    let id = int_arg(args, 0, "ngrpc_serve_bg", span)?;
    SERVERS.with(|m| {
        let mut map = m.borrow_mut();
        match map.get_mut(&id) {
            Some(s) => {
                if !s.niao_routes.is_empty() && !s.has_sync {
                    return Ok(ngrpc_err(
                        span,
                        "serve_bg cannot use Niao handlers; use poll()/serve() or mount_echo() for SyncHandler",
                    ));
                }
                match s.grpc.serve_bg() {
                    Ok(()) => Ok(Value::Bool(true).ref_cell()),
                    Err(e) => Ok(map_grpc(span, e)),
                }
            }
            None => Ok(invalid_handle(span, "server", id)),
        }
    })
}

// >>> ngrpc.poll(s, 10)
// => false
fn ngrpc_poll(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ngrpc_poll", span)?;
    let id = int_arg(args, 0, "ngrpc_poll", span)?;
    let timeout = if args.len() >= 2 {
        let ms = int_arg(args, 1, "ngrpc_poll", span)?;
        if ms < 0 {
            return Err(type_err(span, "ngrpc_poll() timeout_ms must be >= 0"));
        }
        Some(Duration::from_millis(ms as u64))
    } else {
        None
    };
    match with_server_taken(id, span, |s| {
        if s.niao_routes.is_empty() {
            s.grpc.poll(timeout)
        } else {
            let routes = s.niao_routes.clone();
            s.grpc
                .poll_with(timeout, |rpc| dispatch_from_routes(&routes, rpc, span))
        }
    }) {
        Ok(Ok(served)) => Ok(Value::Bool(served).ref_cell()),
        Ok(Err(e)) => Ok(map_grpc(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> ngrpc.serve(s)
fn ngrpc_serve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_serve", span)?;
    let id = int_arg(args, 0, "ngrpc_serve", span)?;
    match with_server_taken(id, span, |s| {
        if s.niao_routes.is_empty() {
            s.grpc.serve()
        } else {
            let routes = s.niao_routes.clone();
            s.grpc
                .serve_with(|rpc| dispatch_from_routes(&routes, rpc, span))
        }
    }) {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(map_grpc(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> ngrpc.stop(s)
// => true
fn ngrpc_stop(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_stop", span)?;
    let id = int_arg(args, 0, "ngrpc_stop", span)?;
    SERVERS.with(|m| match m.borrow().get(&id) {
        Some(s) => {
            s.grpc.stop();
            Ok(Value::Bool(true).ref_cell())
        }
        None => Ok(invalid_handle(span, "server", id)),
    })
}

// >>> ngrpc.close_server(s)
// => true
fn ngrpc_close_server(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ngrpc_close_server", span)?;
    let id = int_arg(args, 0, "ngrpc_close_server", span)?;
    let removed = SERVERS.with(|m| {
        if let Some(s) = m.borrow_mut().remove(&id) {
            s.grpc.stop();
            s.grpc.join_bg();
            true
        } else {
            false
        }
    });
    if removed {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(invalid_handle(span, "server", id))
    }
}

// >>> let s = ngrpc.server(); ngrpc.on(s, "/e/E", "unary", fn(r, m) { r }); let o = ngrpc.invoke_local(s, "/e/E", "hi"); ngrpc.close_server(s); o.status
// => 0
fn ngrpc_invoke_local(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "ngrpc_invoke_local", span)?;
    let id = int_arg(args, 0, "ngrpc_invoke_local", span)?;
    let method = string_arg(args, 1, "ngrpc_invoke_local", span)?;
    let path = match normalize_method_path(&method) {
        Ok(p) => p,
        Err(e) => return Ok(map_grpc(span, e)),
    };
    let messages = if args.len() >= 3 {
        messages_from_value(&args[2].borrow(), "ngrpc_invoke_local", span)?
    } else {
        Vec::new()
    };
    let metadata = if args.len() >= 4 {
        value_to_metadata(&args[3].borrow(), "ngrpc_invoke_local", span)?
    } else {
        HashMap::new()
    };
    let route = SERVERS.with(|m| {
        m.borrow().get(&id).map(|s| {
            s.niao_routes
                .get(&path)
                .map(|(k, h)| (*k, Rc::clone(h)))
        })
    });
    match route {
        None => Ok(invalid_handle(span, "server", id)),
        Some(None) => Ok(ngrpc_err(
            span,
            format!("no Niao handler registered for {path}"),
        )),
        Some(Some((kind, handler))) => {
            let rpc = IncomingRpc {
                method: path,
                kind,
                metadata,
                messages,
            };
            let reply = call_niao_route(&handler, kind, rpc, span);
            Ok(rpc_result_value(&handler_reply_to_rpc(reply)))
        }
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ngrpc_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ngrpc_fns![
    ("ngrpc_status_ok", "status_ok", ngrpc_status_ok),
    ("ngrpc_status_name", "status_name", ngrpc_status_name),
    ("ngrpc_status_code", "status_code", ngrpc_status_code),
    ("ngrpc_method_path", "method_path", ngrpc_method_path),
    ("ngrpc_parse_method", "parse_method", ngrpc_parse_method),
    ("ngrpc_frame", "frame", ngrpc_frame),
    ("ngrpc_unframe", "unframe", ngrpc_unframe),
    ("ngrpc_unframe_all", "unframe_all", ngrpc_unframe_all),
    ("ngrpc_channel", "channel", ngrpc_channel),
    ("ngrpc_close_channel", "close_channel", ngrpc_close_channel),
    ("ngrpc_channel_target", "channel_target", ngrpc_channel_target),
    ("ngrpc_unary", "unary", ngrpc_unary),
    ("ngrpc_open_server_stream", "open_server_stream", ngrpc_open_server_stream),
    ("ngrpc_open_client_stream", "open_client_stream", ngrpc_open_client_stream),
    ("ngrpc_open_bidi", "open_bidi", ngrpc_open_bidi),
    ("ngrpc_send", "send", ngrpc_send),
    ("ngrpc_send_close", "send_close", ngrpc_send_close),
    ("ngrpc_recv", "recv", ngrpc_recv),
    ("ngrpc_finish", "finish", ngrpc_finish),
    ("ngrpc_close_call", "close_call", ngrpc_close_call),
    ("ngrpc_server", "server", ngrpc_server),
    ("ngrpc_addr", "addr", ngrpc_addr),
    ("ngrpc_on", "on", ngrpc_on),
    ("ngrpc_mount_echo", "mount_echo", ngrpc_mount_echo),
    ("ngrpc_serve_bg", "serve_bg", ngrpc_serve_bg),
    ("ngrpc_poll", "poll", ngrpc_poll),
    ("ngrpc_serve", "serve", ngrpc_serve),
    ("ngrpc_stop", "stop", ngrpc_stop),
    ("ngrpc_close_server", "close_server", ngrpc_close_server),
    ("ngrpc_invoke_local", "invoke_local", ngrpc_invoke_local),
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

pub const MODULE_NAME: &str = "ngrpc";
pub const MODULE_PATHS: &[&str] = &["ngrpc", "std/ngrpc"];

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
    fn status_ok_and_method_path() {
        let ok = ngrpc_status_ok(&[], span()).unwrap();
        match &*ok.borrow() {
            Value::Int(0) => {}
            other => panic!("expected 0, got {other:?}"),
        }

        let path = ngrpc_method_path(
            &[
                Value::String("echo.Echo".into()).ref_cell(),
                Value::String("Say".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*path.borrow() {
            Value::String(s) => assert_eq!(s, "/echo.Echo/Say"),
            other => panic!("expected string path, got {other:?}"),
        }
    }
}
