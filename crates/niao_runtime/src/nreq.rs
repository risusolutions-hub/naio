//! Native nreq standard library — ergonomic HTTP client (~requests, httpx).
//!
//! Import with `import "nreq"` (or `import "std/nreq"`).

use crate::{error_value, json_stringify, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_json_core::Value as JsonValue;
use niao_req::{
    basic_auth, bearer, build_multipart, cookie_header_from_map, decode_form_map, default_user_agent,
    download, encode_form_map, execute, join_url, parse_set_cookie, prepare_url, MultipartPart,
    RequestOpts, ReqError, Response, Session,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

const E4450: u32 = codes::E4490_NREQ_ARITY;
const E4451: u32 = codes::E4491_NREQ_ERROR;
const E4452: u32 = codes::E4492_NREQ_TYPE;
const E4453: u32 = codes::E4493_NREQ_INVALID_HANDLE;

thread_local! {
    static SESSIONS: RefCell<HashMap<i64, Session>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn new_id() -> i64 {
    NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4452, msg.into())
}

fn nreq_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4451, "nreq_error", msg.into(), span)
}

fn map_req_err(span: Span, e: ReqError) -> ValueRef {
    nreq_err(span, e.to_string())
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects positive session handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(m) => Some(m.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        Some(Value::String(s)) => matches!(s.as_str(), "true" | "1" | "yes" | "on"),
        _ => default,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn string_map_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> HashMap<String, String> {
    let Some(map) = map else {
        return HashMap::new();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Object(obj)) => {
            let mut out = HashMap::new();
            for (k, v) in obj {
                match &*v.borrow() {
                    Value::String(s) => {
                        out.insert(k, s.clone());
                    }
                    Value::Int(n) => {
                        out.insert(k, n.to_string());
                    }
                    Value::Float(f) => {
                        out.insert(k, f.to_string());
                    }
                    Value::Bool(b) => {
                        out.insert(k, b.to_string());
                    }
                    _ => {}
                }
            }
            out
        }
        _ => HashMap::new(),
    }
}

fn value_to_json_string(v: &Value, span: Span) -> NiaoResult<String> {
    let vr = v.clone().ref_cell();
    let out = json_stringify(&[vr], span)?;
    let owned = out.borrow().clone();
    match owned {
        Value::String(s) => Ok(s),
        _ => Err(type_err(span, "json stringify failed")),
    }
}

fn parse_opts(map: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<RequestOpts> {
    let mut opts = RequestOpts::default();
    let Some(map) = map else {
        return Ok(opts);
    };
    opts.headers = string_map_field(Some(map), "headers");
    opts.params = string_map_field(Some(map), "params");
    opts.cookies = string_map_field(Some(map), "cookies");
    if let Some(s) = string_field(Some(map), "data") {
        opts.data = Some(s);
    } else if let Some(Value::Object(_)) = map.get("data").map(|v| v.borrow().clone()) {
        let form_map = string_map_field(Some(map), "data");
        let btree: BTreeMap<_, _> = form_map.into_iter().collect();
        opts.data = Some(encode_form_map(&btree));
        opts.content_type = Some("application/x-www-form-urlencoded".into());
    }
    if let Some(s) = string_field(Some(map), "json") {
        opts.json = Some(s);
    } else if let Some(jv) = map.get("json") {
        opts.json = Some(value_to_json_string(&jv.borrow(), span)?);
    }
    if let Some(Value::IntArray(bytes)) = map.get("body_bytes").map(|v| v.borrow().clone()) {
        opts.body_bytes = Some(bytes.iter().map(|&b| b as u8).collect());
    }
    if let Some(s) = string_field(Some(map), "content_type") {
        opts.content_type = Some(s);
    }
    if let Some(s) = string_field(Some(map), "user_agent") {
        opts.user_agent = Some(s);
    }
    if let Some(s) = string_field(Some(map), "proxy") {
        opts.proxy = Some(s);
    }
    if let Some(s) = string_field(Some(map), "bearer") {
        opts.bearer = Some(s);
    }
    if let Some(ms) = int_field(Some(map), "timeout_ms") {
        if ms >= 0 {
            opts.timeout_ms = Some(ms as u64);
        }
    }
    if let Some(n) = int_field(Some(map), "max_redirects") {
        if n >= 0 {
            opts.max_redirects = Some(n.min(255) as u8);
        }
    }
    if map.contains_key("allow_redirects") {
        opts.allow_redirects = Some(bool_field(Some(map), "allow_redirects", true));
    }
    if let Some(n) = int_field(Some(map), "retries") {
        if n >= 0 {
            opts.retries = Some(n as u32);
        }
    }
    if let Some(n) = int_field(Some(map), "backoff_ms") {
        if n >= 0 {
            opts.backoff_ms = Some(n as u64);
        }
    }
    if let Some(Value::Array(items)) = map.get("retry_statuses").map(|v| v.borrow().clone()) {
        let mut statuses = Vec::new();
        for it in items {
            if let Value::Int(n) = &*it.borrow() {
                statuses.push(*n as u16);
            }
        }
        opts.retry_statuses = Some(statuses);
    }
    // auth: [user, pass] or {user, pass} / {username, password}
    if let Some(v) = map.get("auth") {
        match &*v.borrow() {
            Value::Array(items) if items.len() >= 2 => {
                let u = match &*items[0].borrow() {
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                };
                let p = match &*items[1].borrow() {
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                };
                opts.auth = Some((u, p));
            }
            Value::Object(o) => {
                let u = o
                    .get("user")
                    .or_else(|| o.get("username"))
                    .and_then(|x| {
                        if let Value::String(s) = &*x.borrow() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                let p = o
                    .get("pass")
                    .or_else(|| o.get("password"))
                    .and_then(|x| {
                        if let Value::String(s) = &*x.borrow() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                opts.auth = Some((u, p));
            }
            _ => {}
        }
    }
    // files: [{name, filename?, content|data, content_type?}, ...]
    if let Some(Value::Array(items)) = map.get("files").map(|v| v.borrow().clone()) {
        for it in items {
            if let Value::Object(part) = &*it.borrow() {
                let name = part
                    .get("name")
                    .and_then(|v| {
                        if let Value::String(s) = &*v.borrow() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "file".into());
                let filename = part.get("filename").and_then(|v| {
                    if let Value::String(s) = &*v.borrow() {
                        Some(s.clone())
                    } else {
                        None
                    }
                });
                let content_type = part.get("content_type").and_then(|v| {
                    if let Value::String(s) = &*v.borrow() {
                        Some(s.clone())
                    } else {
                        None
                    }
                });
                let data = part
                    .get("content")
                    .or_else(|| part.get("data"))
                    .map(|v| match &*v.borrow() {
                        Value::String(s) => s.as_bytes().to_vec(),
                        Value::IntArray(bs) => bs.iter().map(|&b| b as u8).collect(),
                        _ => Vec::new(),
                    })
                    .unwrap_or_default();
                if let Some(fname) = filename {
                    opts.files
                        .push(MultipartPart::file(name, fname, data, content_type));
                } else {
                    opts.files.push(MultipartPart::field(name, data));
                }
            }
        }
    }
    Ok(opts)
}

fn session_from_map(map: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<Session> {
    let mut s = Session::new();
    let Some(map) = map else {
        return Ok(s);
    };
    if let Some(u) = string_field(Some(map), "base_url") {
        s.base_url = u;
    }
    s.headers = string_map_field(Some(map), "headers");
    s.params = string_map_field(Some(map), "params");
    if let Some(ua) = string_field(Some(map), "user_agent") {
        s.user_agent = ua;
    }
    if let Some(p) = string_field(Some(map), "proxy") {
        s.proxy = Some(p);
    }
    if let Some(b) = string_field(Some(map), "bearer") {
        s.bearer = Some(b);
    }
    if let Some(ms) = int_field(Some(map), "timeout_ms") {
        if ms >= 0 {
            s.timeout_ms = ms as u64;
        }
    }
    if let Some(n) = int_field(Some(map), "max_redirects") {
        if n >= 0 {
            s.max_redirects = n.min(255) as u8;
        }
    }
    if map.contains_key("allow_redirects") {
        s.allow_redirects = bool_field(Some(map), "allow_redirects", true);
    }
    if let Some(n) = int_field(Some(map), "retries") {
        if n >= 0 {
            s.retries = n as u32;
        }
    }
    if let Some(n) = int_field(Some(map), "backoff_ms") {
        if n >= 0 {
            s.backoff_ms = n as u64;
        }
    }
    if let Some(v) = map.get("auth") {
        let tmp = HashMap::from([("auth".into(), v.clone())]);
        let parsed = parse_opts(Some(&tmp), span)?;
        s.auth = parsed.auth;
    }
    // seed cookies from {name: value}
    for (k, v) in string_map_field(Some(map), "cookies") {
        let mut c = niao_req::Cookie::new(k, v);
        c.path = "/".into();
        s.cookies.set(c);
    }
    Ok(s)
}

fn response_to_value(resp: Response) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("status".into(), Value::Int(resp.status as i64).ref_cell());
    map.insert("ok".into(), Value::Bool(resp.ok()).ref_cell());
    map.insert("url".into(), Value::String(resp.url.clone()).ref_cell());
    map.insert("reason".into(), Value::String(resp.reason.clone()).ref_cell());
    map.insert("elapsed_ms".into(), Value::Int(resp.elapsed_ms as i64).ref_cell());
    map.insert("body".into(), Value::String(resp.text()).ref_cell());
    map.insert(
        "body_bytes".into(),
        Value::IntArray(resp.body.iter().map(|&b| b as i64).collect()).ref_cell(),
    );
    let mut headers = HashMap::new();
    for (k, v) in &resp.headers {
        headers.insert(k.clone(), Value::String(v.clone()).ref_cell());
    }
    map.insert("headers".into(), Value::Object(headers).ref_cell());
    let cookies: Vec<ValueRef> = resp
        .set_cookies
        .iter()
        .map(|c| Value::String(c.clone()).ref_cell())
        .collect();
    map.insert("set_cookies".into(), Value::Array(cookies).ref_cell());
    Value::Object(map).ref_cell()
}

fn with_session<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&mut Session) -> NiaoResult<ValueRef>,
{
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        match sessions.get_mut(&id) {
            Some(s) => f(s),
            None => Ok(error_value(
                E4453,
                "nreq_error",
                format!("invalid or closed nreq session handle {id}"),
                span,
            )),
        }
    })
}

/// Resolve (session?, url, opts) from verb args.
/// Patterns: (url), (url, opts), (session, url), (session, url, opts)
fn split_call_args(
    args: &[ValueRef],
    name: &str,
    span: Span,
) -> NiaoResult<(Option<i64>, String, RequestOpts)> {
    if args.is_empty() {
        return Err(RuntimeError::at(
            span,
            E4450,
            format!("{name}() expects at least a url"),
        ));
    }
    match &*args[0].borrow() {
        Value::Int(id) if *id > 0 => {
            arity_range(args, 2, 3, name, span)?;
            let url = string_arg(args, 1, name, span)?;
            let opts = parse_opts(optional_object(args, 2).as_ref(), span)?;
            Ok((Some(*id), url, opts))
        }
        Value::String(url) => {
            arity_range(args, 1, 2, name, span)?;
            let opts = parse_opts(optional_object(args, 1).as_ref(), span)?;
            Ok((None, url.clone(), opts))
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects url string or session handle first, got {}",
                other.type_name()
            ),
        )),
    }
}

fn do_verb(method: &str, args: &[ValueRef], name: &str, span: Span) -> NiaoResult<ValueRef> {
    let (sid, url, opts) = split_call_args(args, name, span)?;
    if let Some(id) = sid {
        with_session(id, span, |s| match execute(method, &url, s, &opts) {
            Ok(r) => Ok(response_to_value(r)),
            Err(e) => Ok(map_req_err(span, e)),
        })
    } else {
        let mut s = Session::new();
        match execute(method, &url, &mut s, &opts) {
            Ok(r) => Ok(response_to_value(r)),
            Err(e) => Ok(map_req_err(span, e)),
        }
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nreq_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.encode_form({a: "1"})
    // => "a=1"
    do_verb("GET", args, "nreq_get", span)
}

fn nreq_post(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    do_verb("POST", args, "nreq_post", span)
}

fn nreq_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    do_verb("PUT", args, "nreq_put", span)
}

fn nreq_patch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    do_verb("PATCH", args, "nreq_patch", span)
}

fn nreq_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    do_verb("DELETE", args, "nreq_delete", span)
}

fn nreq_head(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    do_verb("HEAD", args, "nreq_head", span)
}

fn nreq_request(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nreq_request", span)?;
    let method = string_arg(args, 0, "nreq_request", span)?;
    // shift: request(method, url|session, ...)
    let rest: Vec<ValueRef> = args[1..].to_vec();
    do_verb(&method, &rest, "nreq_request", span)
}

fn nreq_session(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let s = nreq.session(); nreq.close(s); true
    // => true
    arity_range(args, 0, 1, "nreq_session", span)?;
    let s = session_from_map(optional_object(args, 0).as_ref(), span)?;
    let id = new_id();
    SESSIONS.with(|m| m.borrow_mut().insert(id, s));
    Ok(Value::Int(id).ref_cell())
}

fn nreq_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nreq_close", span)?;
    let id = handle_arg(args, 0, "nreq_close", span)?;
    let removed = SESSIONS.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn nreq_session_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nreq_session_info", span)?;
    let id = handle_arg(args, 0, "nreq_session_info", span)?;
    with_session(id, span, |s| {
        let mut map = HashMap::new();
        map.insert("base_url".into(), Value::String(s.base_url.clone()).ref_cell());
        map.insert(
            "timeout_ms".into(),
            Value::Int(s.timeout_ms as i64).ref_cell(),
        );
        map.insert("retries".into(), Value::Int(s.retries as i64).ref_cell());
        map.insert(
            "backoff_ms".into(),
            Value::Int(s.backoff_ms as i64).ref_cell(),
        );
        map.insert(
            "max_redirects".into(),
            Value::Int(s.max_redirects as i64).ref_cell(),
        );
        map.insert(
            "allow_redirects".into(),
            Value::Bool(s.allow_redirects).ref_cell(),
        );
        map.insert(
            "user_agent".into(),
            Value::String(s.user_agent.clone()).ref_cell(),
        );
        map.insert(
            "cookie_count".into(),
            Value::Int(s.cookies.len() as i64).ref_cell(),
        );
        if let Some(p) = &s.proxy {
            map.insert("proxy".into(), Value::String(p.clone()).ref_cell());
        }
        Ok(Value::Object(map).ref_cell())
    })
}

fn nreq_json(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.encode_form({x: "y"})
    // => "x=y"
    arity_range(args, 1, 1, "nreq_json", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            let body = m
                .get("body")
                .and_then(|v| {
                    if let Value::String(s) = &*v.borrow() {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            match niao_json_core::parse(&body) {
                Ok(jv) => Ok(json_to_niao(&jv).ref_cell()),
                Err(e) => Ok(nreq_err(span, e.to_string())),
            }
        }
        Value::String(s) => match niao_json_core::parse(s) {
            Ok(jv) => Ok(json_to_niao(&jv).ref_cell()),
            Err(e) => Ok(nreq_err(span, e.to_string())),
        },
        other => Err(type_err(
            span,
            format!("nreq_json() expects response object or string, got {}", other.type_name()),
        )),
    }
}

fn json_to_niao(v: &JsonValue) -> Value {
    match v {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(u) = n.as_u64() {
                Value::Int(u as i64)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::String(n.to_string())
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(items) => {
            Value::Array(items.iter().map(json_to_niao).map(|v| v.ref_cell()).collect())
        }
        JsonValue::Object(o) => {
            let mut out = HashMap::new();
            for (k, v) in o.iter() {
                out.insert(k.to_string(), json_to_niao(v).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn nreq_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nreq_ok", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            let ok = match m.get("ok").map(|v| v.borrow().clone()) {
                Some(Value::Bool(b)) => b,
                Some(Value::Int(n)) => (200..300).contains(&(n as u16)),
                _ => match m.get("status").map(|v| v.borrow().clone()) {
                    Some(Value::Int(n)) => (200..300).contains(&(n as u16)),
                    _ => false,
                },
            };
            Ok(Value::Bool(ok).ref_cell())
        }
        _ => Ok(Value::Bool(false).ref_cell()),
    }
}

fn nreq_raise_for_status(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nreq_raise_for_status", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            let status = match m.get("status").map(|v| v.borrow().clone()) {
                Some(Value::Int(n)) => n as u16,
                _ => 0,
            };
            if (200..300).contains(&status) {
                Ok(args[0].clone())
            } else {
                let body = m
                    .get("body")
                    .and_then(|v| {
                        if let Value::String(s) = &*v.borrow() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                let preview: String = body.chars().take(200).collect();
                Ok(nreq_err(
                    span,
                    format!(
                        "HTTP {status}: {}",
                        if preview.is_empty() {
                            niao_req::reason_phrase(status).into()
                        } else {
                            preview
                        }
                    ),
                ))
            }
        }
        other => Err(type_err(
            span,
            format!(
                "nreq_raise_for_status() expects response object, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nreq_encode_form(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.encode_form({hello: "world"})
    // => "hello=world"
    arity_range(args, 1, 1, "nreq_encode_form", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            let mut btree = BTreeMap::new();
            for (k, v) in m {
                let s = match &*v.borrow() {
                    Value::String(s) => s.clone(),
                    Value::Int(n) => n.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Nil => String::new(),
                    _ => continue,
                };
                btree.insert(k.clone(), s);
            }
            Ok(Value::String(encode_form_map(&btree)).ref_cell())
        }
        Value::String(s) => Ok(Value::String(s.clone()).ref_cell()),
        other => Err(type_err(
            span,
            format!("nreq_encode_form() expects object, got {}", other.type_name()),
        )),
    }
}

fn nreq_decode_form(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.decode_form("a=1&b=2").a
    // => "1"
    arity_range(args, 1, 1, "nreq_decode_form", span)?;
    let s = string_arg(args, 0, "nreq_decode_form", span)?;
    match decode_form_map(&s) {
        Ok(m) => {
            let mut out = HashMap::new();
            for (k, v) in m {
                out.insert(k, Value::String(v).ref_cell());
            }
            Ok(Value::Object(out).ref_cell())
        }
        Err(e) => Ok(map_req_err(span, e)),
    }
}

fn nreq_multipart(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> let m = nreq.multipart([{name: "a", data: "x"}]); m.content_type.contains("multipart")
    // => true
    arity_range(args, 1, 2, "nreq_multipart", span)?;
    let mut parts = Vec::new();
    match &*args[0].borrow() {
        Value::Array(items) => {
            for it in items {
                if let Value::Object(part) = &*it.borrow() {
                    let name = part
                        .get("name")
                        .and_then(|v| {
                            if let Value::String(s) = &*v.borrow() {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| "field".into());
                    let filename = part.get("filename").and_then(|v| {
                        if let Value::String(s) = &*v.borrow() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    });
                    let content_type = part.get("content_type").and_then(|v| {
                        if let Value::String(s) = &*v.borrow() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    });
                    let data = part
                        .get("content")
                        .or_else(|| part.get("data"))
                        .map(|v| match &*v.borrow() {
                            Value::String(s) => s.as_bytes().to_vec(),
                            Value::IntArray(bs) => bs.iter().map(|&b| b as u8).collect(),
                            _ => Vec::new(),
                        })
                        .unwrap_or_default();
                    if let Some(fname) = filename {
                        parts.push(MultipartPart::file(name, fname, data, content_type));
                    } else {
                        parts.push(MultipartPart::field(name, data));
                    }
                }
            }
        }
        other => {
            return Err(type_err(
                span,
                format!("nreq_multipart() expects array of parts, got {}", other.type_name()),
            ))
        }
    }
    let boundary = if args.len() > 1 {
        Some(string_arg(args, 1, "nreq_multipart", span)?)
    } else {
        None
    };
    match build_multipart(&parts, boundary.as_deref()) {
        Ok(mp) => {
            let mut map = HashMap::new();
            map.insert(
                "content_type".into(),
                Value::String(mp.content_type()).ref_cell(),
            );
            map.insert("boundary".into(), Value::String(mp.boundary).ref_cell());
            map.insert(
                "body".into(),
                Value::String(String::from_utf8_lossy(&mp.body).into_owned()).ref_cell(),
            );
            map.insert(
                "body_bytes".into(),
                Value::IntArray(mp.body.iter().map(|&b| b as i64).collect()).ref_cell(),
            );
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_req_err(span, e)),
    }
}

fn nreq_basic_auth(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.basic_auth("u", "p").starts_with("Basic ")
    // => true
    arity_range(args, 2, 2, "nreq_basic_auth", span)?;
    let u = string_arg(args, 0, "nreq_basic_auth", span)?;
    let p = string_arg(args, 1, "nreq_basic_auth", span)?;
    Ok(Value::String(basic_auth(&u, &p)).ref_cell())
}

fn nreq_bearer(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.bearer("tok")
    // => "Bearer tok"
    arity_range(args, 1, 1, "nreq_bearer", span)?;
    let t = string_arg(args, 0, "nreq_bearer", span)?;
    Ok(Value::String(bearer(&t)).ref_cell())
}

fn nreq_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.join("https://ex.com/a/", "b")
    // => "https://ex.com/a/b"
    arity_range(args, 2, 2, "nreq_join", span)?;
    let base = string_arg(args, 0, "nreq_join", span)?;
    let path = string_arg(args, 1, "nreq_join", span)?;
    match join_url(&base, &path) {
        Ok(u) => Ok(Value::String(u).ref_cell()),
        Err(e) => Ok(map_req_err(span, e)),
    }
}

fn nreq_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nreq_url", span)?;
    let base = string_arg(args, 0, "nreq_url", span)?;
    let path = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::String(s) => Some(s.clone()),
            Value::Nil => None,
            Value::Object(_) => None,
            other => {
                return Err(type_err(
                    span,
                    format!("nreq_url() path must be string, got {}", other.type_name()),
                ))
            }
        }
    } else {
        None
    };
    let params_idx = if args.len() == 3 {
        2
    } else if args.len() == 2 {
        if matches!(&*args[1].borrow(), Value::Object(_)) {
            1
        } else {
            usize::MAX
        }
    } else {
        usize::MAX
    };
    let params = if params_idx < args.len() {
        string_map_field(optional_object(args, params_idx).as_ref(), "")
            .into_iter()
            .collect::<Vec<_>>()
    } else if let Some(obj) = optional_object(args, params_idx) {
        // when path omitted and second arg is object — treat as params
        let mut pairs = Vec::new();
        for (k, v) in obj {
            let s = match &*v.borrow() {
                Value::String(s) => s.clone(),
                Value::Int(n) => n.to_string(),
                _ => continue,
            };
            pairs.push((k, s));
        }
        pairs
    } else {
        Vec::new()
    };
    // Fix params extraction for object-as-second-arg
    let params = if params_idx < args.len() {
        match &*args[params_idx].borrow() {
            Value::Object(m) => {
                let mut pairs = Vec::new();
                for (k, v) in m {
                    let s = match &*v.borrow() {
                        Value::String(s) => s.clone(),
                        Value::Int(n) => n.to_string(),
                        Value::Float(f) => f.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => continue,
                    };
                    pairs.push((k.clone(), s));
                }
                pairs
            }
            _ => params,
        }
    } else {
        params
    };
    match prepare_url(&base, path.as_deref(), &params) {
        Ok(u) => Ok(Value::String(u).ref_cell()),
        Err(e) => Ok(map_req_err(span, e)),
    }
}

fn nreq_download(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // Patterns: (url, path), (url, path, opts), (session, url, path), (session, url, path, opts)
    if args.is_empty() {
        return Err(RuntimeError::at(span, E4450, "nreq_download() expects arguments"));
    }
    match &*args[0].borrow() {
        Value::Int(id) if *id > 0 => {
            arity_range(args, 3, 4, "nreq_download", span)?;
            let url = string_arg(args, 1, "nreq_download", span)?;
            let path = string_arg(args, 2, "nreq_download", span)?;
            let opts = parse_opts(optional_object(args, 3).as_ref(), span)?;
            with_session(*id, span, |s| match download("GET", &url, &path, s, &opts) {
                Ok(r) => Ok(response_to_value(r)),
                Err(e) => Ok(map_req_err(span, e)),
            })
        }
        Value::String(url) => {
            arity_range(args, 2, 3, "nreq_download", span)?;
            let path = string_arg(args, 1, "nreq_download", span)?;
            let opts = parse_opts(optional_object(args, 2).as_ref(), span)?;
            let mut s = Session::new();
            match download("GET", url, &path, &mut s, &opts) {
                Ok(r) => Ok(response_to_value(r)),
                Err(e) => Ok(map_req_err(span, e)),
            }
        }
        other => Err(type_err(
            span,
            format!(
                "nreq_download() expects url or session first, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nreq_cookies(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nreq_cookies", span)?;
    let id = handle_arg(args, 0, "nreq_cookies", span)?;
    with_session(id, span, |s| {
        let mut map = HashMap::new();
        for c in s.cookies.all() {
            let mut entry = HashMap::new();
            entry.insert("value".into(), Value::String(c.value.clone()).ref_cell());
            entry.insert("domain".into(), Value::String(c.domain.clone()).ref_cell());
            entry.insert("path".into(), Value::String(c.path.clone()).ref_cell());
            entry.insert("secure".into(), Value::Bool(c.secure).ref_cell());
            entry.insert("http_only".into(), Value::Bool(c.http_only).ref_cell());
            map.insert(c.name.clone(), Value::Object(entry).ref_cell());
        }
        Ok(Value::Object(map).ref_cell())
    })
}

fn nreq_set_cookie(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nreq_set_cookie", span)?;
    let id = handle_arg(args, 0, "nreq_set_cookie", span)?;
    let name = string_arg(args, 1, "nreq_set_cookie", span)?;
    let value = string_arg(args, 2, "nreq_set_cookie", span)?;
    let opt = optional_object(args, 3);
    with_session(id, span, |s| {
        let mut c = niao_req::Cookie::new(name, value);
        if let Some(d) = string_field(opt.as_ref(), "domain") {
            c.domain = d;
        }
        if let Some(p) = string_field(opt.as_ref(), "path") {
            c.path = p;
        } else {
            c.path = "/".into();
        }
        c.secure = bool_field(opt.as_ref(), "secure", false);
        c.http_only = bool_field(opt.as_ref(), "http_only", false);
        s.cookies.set(c);
        Ok(Value::Bool(true).ref_cell())
    })
}

fn nreq_clear_cookies(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "nreq_clear_cookies", span)?;
    let id = handle_arg(args, 0, "nreq_clear_cookies", span)?;
    with_session(id, span, |s| {
        s.cookies.clear();
        Ok(Value::Bool(true).ref_cell())
    })
}

fn nreq_parse_set_cookie(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.parse_set_cookie("a=b; Path=/").name
    // => "a"
    arity_range(args, 1, 1, "nreq_parse_set_cookie", span)?;
    let hdr = string_arg(args, 0, "nreq_parse_set_cookie", span)?;
    match parse_set_cookie(&hdr) {
        Ok(c) => {
            let mut map = HashMap::new();
            map.insert("name".into(), Value::String(c.name).ref_cell());
            map.insert("value".into(), Value::String(c.value).ref_cell());
            map.insert("domain".into(), Value::String(c.domain).ref_cell());
            map.insert("path".into(), Value::String(c.path).ref_cell());
            map.insert("secure".into(), Value::Bool(c.secure).ref_cell());
            map.insert("http_only".into(), Value::Bool(c.http_only).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_req_err(span, e)),
    }
}

fn nreq_cookie_header(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // >>> nreq.cookie_header({a: "1", b: "2"})
    // => "a=1; b=2"
    arity_range(args, 1, 1, "nreq_cookie_header", span)?;
    match &*args[0].borrow() {
        Value::Object(m) => {
            let mut map = HashMap::new();
            for (k, v) in m {
                if let Value::String(s) = &*v.borrow() {
                    map.insert(k.clone(), s.clone());
                }
            }
            Ok(Value::String(cookie_header_from_map(&map)).ref_cell())
        }
        other => Err(type_err(
            span,
            format!("nreq_cookie_header() expects object, got {}", other.type_name()),
        )),
    }
}

fn nreq_default_headers(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "nreq_default_headers", span)?;
    let mut map = HashMap::new();
    map.insert(
        "User-Agent".into(),
        Value::String(default_user_agent().into()).ref_cell(),
    );
    Ok(Value::Object(map).ref_cell())
}

fn nreq_boundary(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "nreq_boundary", span)?;
    Ok(Value::String(niao_req::random_boundary()).ref_cell())
}

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("nreq_get", "get", Rc::new(nreq_get)),
        ("nreq_post", "post", Rc::new(nreq_post)),
        ("nreq_put", "put", Rc::new(nreq_put)),
        ("nreq_patch", "patch", Rc::new(nreq_patch)),
        ("nreq_delete", "delete", Rc::new(nreq_delete)),
        ("nreq_head", "head", Rc::new(nreq_head)),
        ("nreq_request", "request", Rc::new(nreq_request)),
        ("nreq_session", "session", Rc::new(nreq_session)),
        ("nreq_close", "close", Rc::new(nreq_close)),
        ("nreq_session_info", "session_info", Rc::new(nreq_session_info)),
        ("nreq_json", "json", Rc::new(nreq_json)),
        ("nreq_ok", "ok", Rc::new(nreq_ok)),
        ("nreq_raise_for_status", "raise_for_status", Rc::new(nreq_raise_for_status)),
        ("nreq_encode_form", "encode_form", Rc::new(nreq_encode_form)),
        ("nreq_decode_form", "decode_form", Rc::new(nreq_decode_form)),
        ("nreq_multipart", "multipart", Rc::new(nreq_multipart)),
        ("nreq_basic_auth", "basic_auth", Rc::new(nreq_basic_auth)),
        ("nreq_bearer", "bearer", Rc::new(nreq_bearer)),
        ("nreq_join", "join", Rc::new(nreq_join)),
        ("nreq_url", "url", Rc::new(nreq_url)),
        ("nreq_download", "download", Rc::new(nreq_download)),
        ("nreq_cookies", "cookies", Rc::new(nreq_cookies)),
        ("nreq_set_cookie", "set_cookie", Rc::new(nreq_set_cookie)),
        ("nreq_clear_cookies", "clear_cookies", Rc::new(nreq_clear_cookies)),
        ("nreq_parse_set_cookie", "parse_set_cookie", Rc::new(nreq_parse_set_cookie)),
        ("nreq_cookie_header", "cookie_header", Rc::new(nreq_cookie_header)),
        ("nreq_default_headers", "default_headers", Rc::new(nreq_default_headers)),
        ("nreq_boundary", "boundary", Rc::new(nreq_boundary)),
    ]
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nreq";
pub const MODULE_PATHS: &[&str] = &["nreq", "std/nreq"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_form_doctest() {
        let mut m = HashMap::new();
        m.insert("hello".into(), Value::String("world".into()).ref_cell());
        let args = vec![Value::Object(m).ref_cell()];
        let v = nreq_encode_form(&args, Span::dummy()).unwrap();
        match &*v.borrow() {
            Value::String(s) => assert_eq!(s, "hello=world"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn bearer_doctest() {
        let args = vec![Value::String("tok".into()).ref_cell()];
        let v = nreq_bearer(&args, Span::dummy()).unwrap();
        match &*v.borrow() {
            Value::String(s) => assert_eq!(s, "Bearer tok"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn session_lifecycle() {
        let id = nreq_session(&[], Span::dummy()).unwrap();
        let handle = match &*id.borrow() {
            Value::Int(n) => *n,
            _ => panic!("expected handle"),
        };
        let closed = nreq_close(&[Value::Int(handle).ref_cell()], Span::dummy()).unwrap();
        match &*closed.borrow() {
            Value::Bool(true) => {}
            other => panic!("expected true, got {other:?}"),
        }
    }
}
