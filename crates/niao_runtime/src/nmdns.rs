//! Native nmdns standard library — mDNS / DNS-SD discovery & announcement
//! (~zeroconf).
//!
//! Import with `import "nmdns"` (or `import "std/nmdns"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_mdns::{
    build_query, decode_message, encode_message, is_mdns_type, localhost_name,
    normalize_service_type, pack_txt, parse_ip, unpack_txt, DiscoveredService, MdnsClient,
    MdnsError, RecordType, ServiceInfo, MDNS_PORT, DEFAULT_TTL,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::rc::Rc;
use std::time::Duration;

const E4450: u32 = codes::E3450_NMDNS_ARITY;
const E4451: u32 = codes::E3451_NMDNS_ERROR;
const E4452: u32 = codes::E3452_NMDNS_TYPE;
const E4453: u32 = codes::E3453_NMDNS_INVALID_HANDLE;
const E4454: u32 = codes::E3454_NMDNS_DECODE;

enum Handle {
    Client(MdnsClient),
    Service(ServiceInfo),
}

thread_local! {
    static STORE: RefCell<HashMap<i64, Handle>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc(h: Handle) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    STORE.with(|m| m.borrow_mut().insert(id, h));
    id
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4452, msg.into())
}

fn nmdns_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4451, "nmdns_error", msg.into(), span)
}

fn decode_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4454, "nmdns_error", msg.into(), span)
}

fn map_err(span: Span, e: MdnsError) -> ValueRef {
    match e {
        MdnsError::Decode(s) | MdnsError::Encode(s) => decode_err(span, s),
        other => nmdns_err(span, other.to_string()),
    }
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4453,
        "nmdns_error",
        format!("invalid or closed nmdns handle {id}"),
        span,
    )
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an nmdns handle as argument {}, got {}",
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
        Value::Float(f) => *f as i64,
        _ => default,
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

fn string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        Some(Value::Int(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn int_field(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => n,
        Some(Value::Float(f)) => f as i64,
        _ => default,
    }
}

fn props_from_value(span: Span, v: &Value) -> NiaoResult<BTreeMap<String, String>> {
    match v {
        Value::Object(m) => {
            let mut out = BTreeMap::new();
            for (k, vr) in m {
                let s = match &*vr.borrow() {
                    Value::String(s) => s.clone(),
                    Value::Int(n) => n.to_string(),
                    Value::Float(f) => f.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Nil => String::new(),
                    other => {
                        return Err(type_err(
                            span,
                            format!("TXT property '{k}' has unsupported type {}", other.type_name()),
                        ));
                    }
                };
                out.insert(k.clone(), s);
            }
            Ok(out)
        }
        Value::Nil => Ok(BTreeMap::new()),
        other => Err(type_err(
            span,
            format!("properties expects object, got {}", other.type_name()),
        )),
    }
}

fn props_from_map(
    span: Span,
    map: Option<&HashMap<String, ValueRef>>,
) -> NiaoResult<BTreeMap<String, String>> {
    match map.and_then(|m| m.get("properties").or_else(|| m.get("props"))) {
        Some(v) => props_from_value(span, &v.borrow()),
        None => Ok(BTreeMap::new()),
    }
}

fn addrs_from_map(
    span: Span,
    map: Option<&HashMap<String, ValueRef>>,
) -> NiaoResult<Vec<IpAddr>> {
    let Some(map) = map else {
        return Ok(vec![]);
    };
    let Some(v) = map.get("addresses").or_else(|| map.get("addrs")) else {
        return Ok(vec![]);
    };
    match &*v.borrow() {
        Value::Array(items) => {
            let mut out = Vec::new();
            for it in items {
                match &*it.borrow() {
                    Value::String(s) => match parse_ip(s) {
                        Ok(ip) => out.push(ip),
                        Err(e) => {
                            return Err(type_err(span, e.to_string()));
                        }
                    },
                    other => {
                        return Err(type_err(
                            span,
                            format!("address must be string, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::String(s) => Ok(vec![parse_ip(s).map_err(|e| type_err(span, e.to_string()))?]),
        Value::Nil => Ok(vec![]),
        other => Err(type_err(
            span,
            format!("addresses expects array or string, got {}", other.type_name()),
        )),
    }
}

fn props_to_value(props: &BTreeMap<String, String>) -> ValueRef {
    let mut m = HashMap::new();
    for (k, v) in props {
        m.insert(k.clone(), Value::String(v.clone()).ref_cell());
    }
    Value::Object(m).ref_cell()
}

fn addrs_to_value(addrs: &[IpAddr]) -> ValueRef {
    Value::Array(
        addrs
            .iter()
            .map(|a| Value::String(a.to_string()).ref_cell())
            .collect(),
    )
    .ref_cell()
}

fn discovered_to_value(s: &DiscoveredService) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("name".into(), Value::String(s.name.clone()).ref_cell());
    m.insert(
        "type".into(),
        Value::String(s.service_type.clone()).ref_cell(),
    );
    m.insert("fullname".into(), Value::String(s.fullname()).ref_cell());
    m.insert("server".into(), Value::String(s.server.clone()).ref_cell());
    m.insert("host".into(), Value::String(s.server.clone()).ref_cell());
    m.insert("port".into(), Value::Int(s.port as i64).ref_cell());
    m.insert("priority".into(), Value::Int(s.priority as i64).ref_cell());
    m.insert("weight".into(), Value::Int(s.weight as i64).ref_cell());
    m.insert("ttl".into(), Value::Int(s.ttl as i64).ref_cell());
    m.insert("addresses".into(), addrs_to_value(&s.addresses));
    m.insert("properties".into(), props_to_value(&s.properties));
    Value::Object(m).ref_cell()
}

fn service_to_value(s: &ServiceInfo) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("name".into(), Value::String(s.name.clone()).ref_cell());
    m.insert(
        "type".into(),
        Value::String(s.service_type.clone()).ref_cell(),
    );
    m.insert("fullname".into(), Value::String(s.fullname()).ref_cell());
    m.insert("host".into(), Value::String(s.host.clone()).ref_cell());
    m.insert("server".into(), Value::String(s.host.clone()).ref_cell());
    m.insert("port".into(), Value::Int(s.port as i64).ref_cell());
    m.insert("priority".into(), Value::Int(s.priority as i64).ref_cell());
    m.insert("weight".into(), Value::Int(s.weight as i64).ref_cell());
    m.insert("ttl".into(), Value::Int(s.ttl as i64).ref_cell());
    m.insert("addresses".into(), addrs_to_value(&s.addresses));
    m.insert("properties".into(), props_to_value(&s.properties));
    Value::Object(m).ref_cell()
}

fn with_client<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&MdnsClient) -> Result<T, MdnsError>,
) -> NiaoResult<ValueRef>
where
    T: IntoNiao,
{
    STORE.with(|store| {
        let store = store.borrow();
        match store.get(&id) {
            Some(Handle::Client(c)) => match f(c) {
                Ok(v) => Ok(v.into_niao()),
                Err(e) => Ok(map_err(span, e)),
            },
            Some(Handle::Service(_)) => Ok(nmdns_err(
                span,
                format!("handle {id} is a service, expected open() client"),
            )),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn with_service_mut(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut ServiceInfo) -> Result<(), MdnsError>,
) -> NiaoResult<ValueRef> {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        match store.get_mut(&id) {
            Some(Handle::Service(s)) => match f(s) {
                Ok(()) => Ok(Value::Bool(true).ref_cell()),
                Err(e) => Ok(map_err(span, e)),
            },
            Some(Handle::Client(_)) => Ok(nmdns_err(
                span,
                format!("handle {id} is a client, expected service() handle"),
            )),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn with_service<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&ServiceInfo) -> T,
) -> NiaoResult<ValueRef>
where
    T: IntoNiao,
{
    STORE.with(|store| {
        let store = store.borrow();
        match store.get(&id) {
            Some(Handle::Service(s)) => Ok(f(s).into_niao()),
            Some(Handle::Client(_)) => Ok(nmdns_err(
                span,
                format!("handle {id} is a client, expected service() handle"),
            )),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

trait IntoNiao {
    fn into_niao(self) -> ValueRef;
}

impl IntoNiao for ValueRef {
    fn into_niao(self) -> ValueRef {
        self
    }
}

impl IntoNiao for bool {
    fn into_niao(self) -> ValueRef {
        Value::Bool(self).ref_cell()
    }
}

impl IntoNiao for i64 {
    fn into_niao(self) -> ValueRef {
        Value::Int(self).ref_cell()
    }
}

impl IntoNiao for String {
    fn into_niao(self) -> ValueRef {
        Value::String(self).ref_cell()
    }
}

impl IntoNiao for () {
    fn into_niao(self) -> ValueRef {
        Value::Bool(true).ref_cell()
    }
}

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                match &*it.borrow() {
                    Value::Int(n) if (0..=255).contains(n) => out.push(*n as u8),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() byte arrays must contain 0..=255 ints, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects bytes/array/string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn bytes_to_value(b: Vec<u8>) -> ValueRef {
    Value::ByteArray(b).ref_cell()
}

fn msg_to_value(msg: &niao_mdns::DnsMessage) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("id".into(), Value::Int(msg.id as i64).ref_cell());
    m.insert("flags".into(), Value::Int(msg.flags as i64).ref_cell());
    m.insert(
        "is_response".into(),
        Value::Bool(msg.is_response()).ref_cell(),
    );
    let qs: Vec<ValueRef> = msg
        .questions
        .iter()
        .map(|q| {
            let mut qm = HashMap::new();
            qm.insert("name".into(), Value::String(q.name.clone()).ref_cell());
            qm.insert(
                "type".into(),
                Value::String(q.qtype.name()).ref_cell(),
            );
            qm.insert("class".into(), Value::Int(q.qclass as i64).ref_cell());
            Value::Object(qm).ref_cell()
        })
        .collect();
    m.insert("questions".into(), Value::Array(qs).ref_cell());
    fn rr_list(rrs: &[niao_mdns::ResourceRecord]) -> ValueRef {
        Value::Array(
            rrs.iter()
                .map(|rr| {
                    let mut rm = HashMap::new();
                    rm.insert("name".into(), Value::String(rr.name.clone()).ref_cell());
                    rm.insert("type".into(), Value::String(rr.rtype.name()).ref_cell());
                    rm.insert("class".into(), Value::Int(rr.class as i64).ref_cell());
                    rm.insert("ttl".into(), Value::Int(rr.ttl as i64).ref_cell());
                    rm.insert("rdata".into(), Value::ByteArray(rr.rdata.clone()).ref_cell());
                    Value::Object(rm).ref_cell()
                })
                .collect(),
        )
        .ref_cell()
    }
    m.insert("answers".into(), rr_list(&msg.answers));
    m.insert("authorities".into(), rr_list(&msg.authorities));
    m.insert("additionals".into(), rr_list(&msg.additionals));
    Value::Object(m).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nmdns.open()
fn nmdns_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nmdns_open", span)?;
    match MdnsClient::open() {
        Ok(c) => Ok(Value::Int(alloc(Handle::Client(c))).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmdns.close(h)
fn nmdns_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_close", span)?;
    let id = handle_arg(args, 0, "nmdns_close", span)?;
    let existed = STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(existed).ref_cell())
}

// >>> nmdns.service("Demo", "_http._tcp", 8080)
fn nmdns_service(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nmdns_service", span)?;
    let name = string_arg(args, 0, "nmdns_service", span)?;
    let ty = string_arg(args, 1, "nmdns_service", span)?;
    let port = int_arg(args, 2, "nmdns_service", span)?;
    if !(0..=65535).contains(&port) {
        return Ok(nmdns_err(span, "port must be 0..=65535"));
    }
    let opts = optional_object(args, 3);
    let host = opts.as_ref().and_then(|m| string_field(m, "host"));
    let props = props_from_map(span, opts.as_ref())?;
    let addrs = addrs_from_map(span, opts.as_ref())?;
    let priority = opts
        .as_ref()
        .map(|m| int_field(m, "priority", 0))
        .unwrap_or(0) as u16;
    let weight = opts
        .as_ref()
        .map(|m| int_field(m, "weight", 0))
        .unwrap_or(0) as u16;
    let ttl = opts
        .as_ref()
        .map(|m| int_field(m, "ttl", DEFAULT_TTL as i64))
        .unwrap_or(DEFAULT_TTL as i64) as u32;
    match ServiceInfo::new(
        name,
        ty,
        port as u16,
        host,
        addrs,
        props,
        priority,
        weight,
        ttl,
    ) {
        Ok(s) => Ok(Value::Int(alloc(Handle::Service(s))).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nmdns_register(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmdns_register", span)?;
    let zc = handle_arg(args, 0, "nmdns_register", span)?;
    let svc = handle_arg(args, 1, "nmdns_register", span)?;
    STORE.with(|store| {
        let store = store.borrow();
        let client = match store.get(&zc) {
            Some(Handle::Client(c)) => c,
            Some(_) => return Ok(nmdns_err(span, "register() expects open() client as arg 1")),
            None => return Ok(invalid_handle(span, zc)),
        };
        let service = match store.get(&svc) {
            Some(Handle::Service(s)) => s,
            Some(_) => return Ok(nmdns_err(span, "register() expects service() handle as arg 2")),
            None => return Ok(invalid_handle(span, svc)),
        };
        match client.register(service) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

fn nmdns_unregister(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmdns_unregister", span)?;
    let zc = handle_arg(args, 0, "nmdns_unregister", span)?;
    let svc = handle_arg(args, 1, "nmdns_unregister", span)?;
    STORE.with(|store| {
        let store = store.borrow();
        let client = match store.get(&zc) {
            Some(Handle::Client(c)) => c,
            Some(_) => return Ok(nmdns_err(span, "unregister() expects open() client as arg 1")),
            None => return Ok(invalid_handle(span, zc)),
        };
        let service = match store.get(&svc) {
            Some(Handle::Service(s)) => s,
            Some(_) => {
                return Ok(nmdns_err(
                    span,
                    "unregister() expects service() handle as arg 2",
                ))
            }
            None => return Ok(invalid_handle(span, svc)),
        };
        match client.unregister(service) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

fn nmdns_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmdns_update", span)?;
    let zc = handle_arg(args, 0, "nmdns_update", span)?;
    let svc = handle_arg(args, 1, "nmdns_update", span)?;
    STORE.with(|store| {
        let store = store.borrow();
        let client = match store.get(&zc) {
            Some(Handle::Client(c)) => c,
            Some(_) => return Ok(nmdns_err(span, "update() expects open() client as arg 1")),
            None => return Ok(invalid_handle(span, zc)),
        };
        let service = match store.get(&svc) {
            Some(Handle::Service(s)) => s,
            Some(_) => return Ok(nmdns_err(span, "update() expects service() handle as arg 2")),
            None => return Ok(invalid_handle(span, svc)),
        };
        match client.update(service) {
            Ok(()) => Ok(Value::Bool(true).ref_cell()),
            Err(e) => Ok(map_err(span, e)),
        }
    })
}

// >>> let zc = nmdns.open(); nmdns.browse(zc, "_http._tcp", 200)
fn nmdns_browse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmdns_browse", span)?;
    let zc = handle_arg(args, 0, "nmdns_browse", span)?;
    let ty = string_arg(args, 1, "nmdns_browse", span)?;
    let timeout_ms = optional_int(args, 2, 1000).max(0) as u64;
    with_client(zc, span, |c| {
        let list = c.browse(&ty, Duration::from_millis(timeout_ms))?;
        let arr: Vec<ValueRef> = list.iter().map(discovered_to_value).collect();
        Ok(Value::Array(arr).ref_cell())
    })
}

fn nmdns_resolve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nmdns_resolve", span)?;
    let zc = handle_arg(args, 0, "nmdns_resolve", span)?;
    let name = string_arg(args, 1, "nmdns_resolve", span)?;
    let ty = string_arg(args, 2, "nmdns_resolve", span)?;
    let timeout_ms = optional_int(args, 3, 1000).max(0) as u64;
    with_client(zc, span, |c| {
        match c.resolve(&name, &ty, Duration::from_millis(timeout_ms))? {
            Some(s) => Ok(discovered_to_value(&s)),
            None => Ok(Value::Nil.ref_cell()),
        }
    })
}

fn nmdns_get_service_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    // zeroconf naming: get_service_info(type, name) — we take (zc, type, name, timeout?)
    arity_range(args, 3, 4, "nmdns_get_service_info", span)?;
    let zc = handle_arg(args, 0, "nmdns_get_service_info", span)?;
    let ty = string_arg(args, 1, "nmdns_get_service_info", span)?;
    let name = string_arg(args, 2, "nmdns_get_service_info", span)?;
    let timeout_ms = optional_int(args, 3, 1000).max(0) as u64;
    with_client(zc, span, |c| {
        match c.resolve(&name, &ty, Duration::from_millis(timeout_ms))? {
            Some(s) => Ok(discovered_to_value(&s)),
            None => Ok(Value::Nil.ref_cell()),
        }
    })
}

// >>> nmdns.info(svc)
fn nmdns_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_info", span)?;
    let id = handle_arg(args, 0, "nmdns_info", span)?;
    with_service(id, span, service_to_value)
}

fn nmdns_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_name", span)?;
    let id = handle_arg(args, 0, "nmdns_name", span)?;
    with_service(id, span, |s| s.name.clone())
}

fn nmdns_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_type", span)?;
    let id = handle_arg(args, 0, "nmdns_type", span)?;
    with_service(id, span, |s| s.service_type.clone())
}

fn nmdns_port(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_port", span)?;
    let id = handle_arg(args, 0, "nmdns_port", span)?;
    with_service(id, span, |s| s.port as i64)
}

fn nmdns_host(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_host", span)?;
    let id = handle_arg(args, 0, "nmdns_host", span)?;
    with_service(id, span, |s| s.host.clone())
}

fn nmdns_fullname(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_fullname", span)?;
    let id = handle_arg(args, 0, "nmdns_fullname", span)?;
    with_service(id, span, |s| s.fullname())
}

fn nmdns_addresses(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_addresses", span)?;
    let id = handle_arg(args, 0, "nmdns_addresses", span)?;
    with_service(id, span, |s| addrs_to_value(&s.addresses))
}

fn nmdns_properties(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_properties", span)?;
    let id = handle_arg(args, 0, "nmdns_properties", span)?;
    with_service(id, span, |s| props_to_value(&s.properties))
}

// >>> nmdns.set_property(svc, "k", "v")
fn nmdns_set_property(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmdns_set_property", span)?;
    let id = handle_arg(args, 0, "nmdns_set_property", span)?;
    let key = string_arg(args, 1, "nmdns_set_property", span)?;
    let val = match &*args[2].borrow() {
        Value::String(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Nil => String::new(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "nmdns_set_property() value must be string/int/bool/nil, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    with_service_mut(id, span, |s| s.set_property(key, val))
}

// >>> nmdns.add_address(svc, "127.0.0.1")
fn nmdns_add_address(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmdns_add_address", span)?;
    let id = handle_arg(args, 0, "nmdns_add_address", span)?;
    let ip_s = string_arg(args, 1, "nmdns_add_address", span)?;
    let ip = match parse_ip(&ip_s) {
        Ok(ip) => ip,
        Err(e) => return Ok(map_err(span, e)),
    };
    with_service_mut(id, span, |s| {
        s.add_address(ip);
        Ok(())
    })
}

// >>> nmdns.service_type("_http._tcp")
fn nmdns_service_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_service_type", span)?;
    let raw = string_arg(args, 0, "nmdns_service_type", span)?;
    match normalize_service_type(&raw) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmdns.is_mdns_type("_http._tcp")
fn nmdns_is_mdns_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_is_mdns_type", span)?;
    let s = string_arg(args, 0, "nmdns_is_mdns_type", span)?;
    Ok(Value::Bool(is_mdns_type(&s)).ref_cell())
}

// >>> nmdns.localhost()
fn nmdns_localhost(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nmdns_localhost", span)?;
    Ok(Value::String(localhost_name()).ref_cell())
}

// >>> nmdns.mdns_group()
fn nmdns_mdns_group(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nmdns_mdns_group", span)?;
    Ok(Value::String("224.0.0.251".into()).ref_cell())
}

// >>> nmdns.mdns_port()
fn nmdns_mdns_port(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nmdns_mdns_port", span)?;
    Ok(Value::Int(MDNS_PORT as i64).ref_cell())
}

// >>> nmdns.pack_txt({path: "/"})
fn nmdns_pack_txt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_pack_txt", span)?;
    let props = props_from_value(span, &args[0].borrow())?;
    match pack_txt(&props) {
        Ok(b) => Ok(bytes_to_value(b)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nmdns_unpack_txt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_unpack_txt", span)?;
    let bytes = bytes_arg(args, 0, "nmdns_unpack_txt", span)?;
    match unpack_txt(&bytes) {
        Ok(m) => Ok(props_to_value(&m)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmdns.encode_query("_http._tcp.local.", "PTR")
fn nmdns_encode_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmdns_encode_query", span)?;
    let name = string_arg(args, 0, "nmdns_encode_query", span)?;
    let qtype = if args.len() >= 2 {
        RecordType::parse(&string_arg(args, 1, "nmdns_encode_query", span)?)
    } else {
        RecordType::Ptr
    };
    match build_query(&name, qtype) {
        Ok(b) => Ok(bytes_to_value(b)),
        Err(e) => Ok(map_err(span, e)),
    }
}

fn nmdns_decode_message(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_decode_message", span)?;
    let bytes = bytes_arg(args, 0, "nmdns_decode_message", span)?;
    match decode_message(&bytes) {
        Ok(msg) => Ok(msg_to_value(&msg)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nmdns.encode_response(svc)
fn nmdns_encode_response(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmdns_encode_response", span)?;
    let id = handle_arg(args, 0, "nmdns_encode_response", span)?;
    STORE.with(|store| {
        let store = store.borrow();
        match store.get(&id) {
            Some(Handle::Service(s)) => match s.to_response_message(false) {
                Ok(msg) => match encode_message(&msg) {
                    Ok(b) => Ok(bytes_to_value(b)),
                    Err(e) => Ok(map_err(span, e)),
                },
                Err(e) => Ok(map_err(span, e)),
            },
            Some(_) => Ok(nmdns_err(
                span,
                "encode_response() expects a service() handle",
            )),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

macro_rules! nmdns_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmdns_fns![
    ("nmdns_open", "open", nmdns_open),
    ("nmdns_close", "close", nmdns_close),
    ("nmdns_service", "service", nmdns_service),
    ("nmdns_register", "register", nmdns_register),
    ("nmdns_unregister", "unregister", nmdns_unregister),
    ("nmdns_update", "update", nmdns_update),
    ("nmdns_browse", "browse", nmdns_browse),
    ("nmdns_resolve", "resolve", nmdns_resolve),
    ("nmdns_get_service_info", "get_service_info", nmdns_get_service_info),
    ("nmdns_info", "info", nmdns_info),
    ("nmdns_name", "name", nmdns_name),
    ("nmdns_type", "type", nmdns_type),
    ("nmdns_port", "port", nmdns_port),
    ("nmdns_host", "host", nmdns_host),
    ("nmdns_fullname", "fullname", nmdns_fullname),
    ("nmdns_addresses", "addresses", nmdns_addresses),
    ("nmdns_properties", "properties", nmdns_properties),
    ("nmdns_set_property", "set_property", nmdns_set_property),
    ("nmdns_add_address", "add_address", nmdns_add_address),
    ("nmdns_service_type", "service_type", nmdns_service_type),
    ("nmdns_is_mdns_type", "is_mdns_type", nmdns_is_mdns_type),
    ("nmdns_localhost", "localhost", nmdns_localhost),
    ("nmdns_mdns_group", "mdns_group", nmdns_mdns_group),
    ("nmdns_mdns_port", "mdns_port", nmdns_mdns_port),
    ("nmdns_pack_txt", "pack_txt", nmdns_pack_txt),
    ("nmdns_unpack_txt", "unpack_txt", nmdns_unpack_txt),
    ("nmdns_encode_query", "encode_query", nmdns_encode_query),
    ("nmdns_decode_message", "decode_message", nmdns_decode_message),
    ("nmdns_encode_response", "encode_response", nmdns_encode_response),
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

pub const MODULE_NAME: &str = "nmdns";
pub const MODULE_PATHS: &[&str] = &["nmdns", "std/nmdns"];

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
    fn open_doctest() {
        let h = nmdns_open(&[], span()).unwrap();
        assert!(matches!(&*h.borrow(), Value::Int(_)));
        let _ = nmdns_close(&[h], span()).unwrap();
    }

    #[test]
    fn service_doctest() {
        let s = nmdns_service(
            &[
                Value::String("Demo".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(8080).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let name = nmdns_name(&[s.clone()], span()).unwrap();
        assert!(matches!(&*name.borrow(), Value::String(ref x) if x == "Demo"));
        let full = nmdns_fullname(&[s.clone()], span()).unwrap();
        assert!(matches!(
            &*full.borrow(),
            Value::String(ref x) if x == "Demo._http._tcp.local."
        ));
        let _ = nmdns_close(&[s], span()).unwrap();
    }

    #[test]
    fn pack_txt_doctest() {
        let mut m = HashMap::new();
        m.insert("path".into(), Value::String("/".into()).ref_cell());
        let packed = nmdns_pack_txt(&[Value::Object(m).ref_cell()], span()).unwrap();
        let back = nmdns_unpack_txt(&[packed], span()).unwrap();
        let borrowed = back.borrow();
        match &*borrowed {
            Value::Object(o) => {
                let path = o.get("path").map(|v| v.borrow());
                assert!(matches!(
                    path.as_deref(),
                    Some(Value::String(ref x)) if x == "/"
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn encode_query_roundtrip() {
        let q = nmdns_encode_query(
            &[
                Value::String("_http._tcp.local.".into()).ref_cell(),
                Value::String("PTR".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let msg = nmdns_decode_message(&[q], span()).unwrap();
        let borrowed = msg.borrow();
        match &*borrowed {
            Value::Object(m) => {
                let ir = m.get("is_response").map(|v| v.borrow());
                assert!(matches!(ir.as_deref(), Some(Value::Bool(false))));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn service_type_helper() {
        let t = nmdns_service_type(&[Value::String("_http._tcp".into()).ref_cell()], span())
            .unwrap();
        assert!(matches!(
            &*t.borrow(),
            Value::String(ref x) if x == "_http._tcp.local."
        ));
    }

    #[test]
    fn invalid_handle() {
        let v = nmdns_name(&[Value::Int(999999).ref_cell()], span()).unwrap();
        let s = format!("{:?}", *v.borrow());
        assert!(s.contains("nmdns") || s.contains("Error") || s.contains("error") || s.contains("invalid"));
    }

    #[test]
    fn bad_port() {
        let v = nmdns_service(
            &[
                Value::String("X".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(70000).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let s = format!("{:?}", *v.borrow());
        assert!(s.contains("port") || s.contains("Error") || s.contains("nmdns"));
    }

    #[test]
    fn empty_name_error() {
        let v = nmdns_service(
            &[
                Value::String("".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(80).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let s = format!("{:?}", *v.borrow());
        assert!(s.contains("empty") || s.contains("Error") || s.contains("nmdns"));
    }

    // >>> nmdns.is_mdns_type("_http._tcp")
    #[test]
    fn is_mdns_type_doctest() {
        let v = nmdns_is_mdns_type(&[Value::String("_http._tcp".into()).ref_cell()], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Bool(true)));
    }

    // >>> nmdns.mdns_port()
    #[test]
    fn mdns_port_doctest() {
        let v = nmdns_mdns_port(&[], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(5353)));
    }

    // >>> nmdns.mdns_group()
    #[test]
    fn mdns_group_doctest() {
        let v = nmdns_mdns_group(&[], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::String(ref s) if s == "224.0.0.251"));
    }

    // >>> nmdns.localhost()
    #[test]
    fn localhost_doctest() {
        let v = nmdns_localhost(&[], span()).unwrap();
        match &*v.borrow() {
            Value::String(s) => assert!(s.ends_with(".local.")),
            other => panic!("{other:?}"),
        }
    }

    // >>> nmdns.info(svc)
    #[test]
    fn info_doctest() {
        let s = nmdns_service(
            &[
                Value::String("Info".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(80).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let info = nmdns_info(&[s.clone()], span()).unwrap();
        match &*info.borrow() {
            Value::Object(m) => {
                let name = m.get("name").map(|v| v.borrow());
                assert!(matches!(name.as_deref(), Some(Value::String(ref x)) if x == "Info"));
            }
            other => panic!("{other:?}"),
        }
        let _ = nmdns_close(&[s], span()).unwrap();
    }

    // >>> nmdns.encode_response(svc)
    #[test]
    fn encode_response_doctest() {
        let s = nmdns_service(
            &[
                Value::String("Enc".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(80).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let wire = nmdns_encode_response(&[s.clone()], span()).unwrap();
        match &*wire.borrow() {
            Value::ByteArray(b) => assert!(b.len() > 12),
            other => panic!("{other:?}"),
        }
        let _ = nmdns_close(&[s], span()).unwrap();
    }

    // >>> nmdns.set_property(svc, "k", "v")
    #[test]
    fn set_property_doctest() {
        let s = nmdns_service(
            &[
                Value::String("Mut".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(9).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let ok = nmdns_set_property(
            &[
                s.clone(),
                Value::String("k".into()).ref_cell(),
                Value::String("v".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));
        let _ = nmdns_close(&[s], span()).unwrap();
    }

    // >>> nmdns.add_address(svc, "127.0.0.1")
    #[test]
    fn add_address_doctest() {
        let s = nmdns_service(
            &[
                Value::String("Addr".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(9).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let ok = nmdns_add_address(
            &[s.clone(), Value::String("127.0.0.1".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));
        let _ = nmdns_close(&[s], span()).unwrap();
    }

    #[test]
    fn register_wrong_handle_types() {
        let zc = nmdns_open(&[], span()).unwrap();
        let svc = nmdns_service(
            &[
                Value::String("R".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(1).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        // swap handles — expect catchable error
        let bad = nmdns_register(&[svc.clone(), zc.clone()], span()).unwrap();
        let s = format!("{:?}", *bad.borrow());
        assert!(s.contains("client") || s.contains("nmdns") || s.contains("Error"));
        let _ = nmdns_close(&[zc, svc], span()).unwrap();
    }

    #[test]
    fn accessor_port_host() {
        let s = nmdns_service(
            &[
                Value::String("Acc".into()).ref_cell(),
                Value::String("_http._tcp".into()).ref_cell(),
                Value::Int(4242).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let port = nmdns_port(&[s.clone()], span()).unwrap();
        assert!(matches!(&*port.borrow(), Value::Int(4242)));
        let ty = nmdns_type(&[s.clone()], span()).unwrap();
        assert!(matches!(
            &*ty.borrow(),
            Value::String(ref x) if x == "_http._tcp.local."
        ));
        let _ = nmdns_close(&[s], span()).unwrap();
    }
}
