//! Native nipaddr standard library — IPv4/IPv6 addresses, CIDR networks,
//! ranges, subnet math, membership checks (~ipaddress).
//!
//! Import with `import "nipaddr"` (or `import "std/nipaddr"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_ipaddr::{
    add_v4, add_v6, address_exclude_v4, address_exclude_v6, broadcast_v4, collapse_networks,
    collect_hosts_v4, collect_hosts_v6, compare_v4, compare_v6, contains_many, entity_contains,
    entity_overlaps, filter_containing, hostmask_v4, iface_ip, iface_network, ipv6_compressed,
    ipv6_exploded, netmask_v4, num_addresses_v4, num_addresses_v6, parse_address, parse_interface,
    parse_ipv4_address, parse_ipv4_network, parse_ipv6_address, parse_ipv6_network, parse_network,
    prefix_to_hostmask_v4, prefix_to_netmask_v4, reverse_ptr_v4, reverse_ptr_v6, subnet_of,
    subnets_v4, subnets_v6, summarize_range, supernet_of, supernet_v4, supernet_v6, valid_address,
    valid_interface, valid_network, v4_is_global, v4_is_link_local, v4_is_loopback, v4_is_multicast,
    v4_is_private, v4_is_reserved, v4_is_unspecified, v6_is_global, v6_is_link_local, v6_is_loopback,
    v6_is_multicast, v6_is_private, v6_is_reserved, v6_is_site_local, v6_is_unspecified,
    with_hostmask_v4, with_netmask_v4, with_prefixlen, DEFAULT_MAX_HOSTS, IpEntity, IpError,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3484: u32 = codes::E3484_NIPADDR_ARITY;
const E3485: u32 = codes::E3485_NIPADDR_ERROR;
const E3486: u32 = codes::E3486_NIPADDR_TYPE;
const E3487: u32 = codes::E3487_NIPADDR_INVALID_HANDLE;

thread_local! {
    static STORE: RefCell<HashMap<i64, IpEntity>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc(entity: IpEntity) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    STORE.with(|m| m.borrow_mut().insert(id, entity));
    id
}

fn with_entity<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&IpEntity) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    STORE.with(|m| {
        match m.borrow().get(&id) {
            Some(e) => Ok(Ok(f(e))),
            None => Ok(Err(error_value(
                E3487,
                "nipaddr_error",
                format!("invalid or closed nipaddr handle {id}"),
                span,
            ))),
        }
    })
}

fn get_entity(id: i64, span: Span) -> NiaoResult<IpEntity> {
    match with_entity(id, span, |e| e.clone())? {
        Ok(e) => Ok(e),
        Err(v) => {
            let msg = match &*v.borrow() {
                Value::Object(m) => m
                    .get("message")
                    .map(|x| match &*x.borrow() {
                        Value::String(s) => s.clone(),
                        _ => "nipaddr error".into(),
                    })
                    .unwrap_or_else(|| "nipaddr error".into()),
                _ => "nipaddr error".into(),
            };
            Err(RuntimeError::at(span, E3485, msg))
        }
    }
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3486, msg.into())
}

fn nipaddr_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3485, "nipaddr_error", msg.into(), span)
}

fn ip_err(span: Span, e: IpError) -> ValueRef {
    nipaddr_err(span, e.to_string())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3484,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3484,
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
                "{name}() expects a nipaddr handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_bool(args: &[ValueRef], idx: usize, default: bool) -> bool {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        _ => default,
    }
}

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn ok_handle(entity: IpEntity) -> NiaoResult<ValueRef> {
    Ok(Value::Int(alloc(entity)).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn bytes_val(bytes: &[u8]) -> NiaoResult<ValueRef> {
    Ok(Value::ByteArray(bytes.to_vec()).ref_cell())
}

fn handles_from_array(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(type_err(
                    span,
                    format!(
                        "{name}() array item {} must be nipaddr handle, got {}",
                        i + 1,
                        other.type_name()
                    ),
                )),
            })
            .collect(),
        Value::Nil => Ok(Vec::new()),
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

fn entities_from_handles(ids: &[i64], span: Span) -> NiaoResult<Vec<IpEntity>> {
    ids.iter().map(|id| get_entity(*id, span)).collect()
}

fn handles_to_array(ids: &[i64]) -> NiaoResult<ValueRef> {
    Ok(Value::Array(
        ids.iter().map(|id| Value::Int(*id).ref_cell()).collect(),
    )
    .ref_cell())
}

fn parse_result(span: Span, r: Result<IpEntity, IpError>) -> NiaoResult<ValueRef> {
    match r {
        Ok(e) => ok_handle(e),
        Err(e) => Ok(ip_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

// >>> nipaddr.address("192.168.1.1")
fn nipaddr_address(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_address", span)?;
    parse_result(span, parse_address(&string_arg(args, 0, "nipaddr_address", span)?))
}

fn nipaddr_ipv4(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_ipv4", span)?;
    match parse_ipv4_address(&string_arg(args, 0, "nipaddr_ipv4", span)?) {
        Ok(a) => ok_handle(IpEntity::V4Addr(a)),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_ipv6(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_ipv6", span)?;
    match parse_ipv6_address(&string_arg(args, 0, "nipaddr_ipv6", span)?) {
        Ok(a) => ok_handle(IpEntity::V6Addr(a)),
        Err(e) => Ok(ip_err(span, e)),
    }
}

// >>> nipaddr.network("10.0.0.0/8", true)
fn nipaddr_network(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nipaddr_network", span)?;
    let strict = optional_bool(args, 1, true);
    parse_result(
        span,
        parse_network(&string_arg(args, 0, "nipaddr_network", span)?, strict),
    )
}

fn nipaddr_ipv4_network(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nipaddr_ipv4_network", span)?;
    let strict = optional_bool(args, 1, true);
    match parse_ipv4_network(&string_arg(args, 0, "nipaddr_ipv4_network", span)?, strict) {
        Ok(n) => ok_handle(IpEntity::V4Net(n)),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_ipv6_network(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nipaddr_ipv6_network", span)?;
    let strict = optional_bool(args, 1, true);
    match parse_ipv6_network(&string_arg(args, 0, "nipaddr_ipv6_network", span)?, strict) {
        Ok(n) => ok_handle(IpEntity::V6Net(n)),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_interface(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_interface", span)?;
    parse_result(span, parse_interface(&string_arg(args, 0, "nipaddr_interface", span)?))
}

fn nipaddr_valid_address(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_valid_address", span)?;
    bool_val(valid_address(&string_arg(args, 0, "nipaddr_valid_address", span)?))
}

fn nipaddr_valid_network(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nipaddr_valid_network", span)?;
    let strict = optional_bool(args, 1, true);
    bool_val(valid_network(
        &string_arg(args, 0, "nipaddr_valid_network", span)?,
        strict,
    ))
}

fn nipaddr_valid_interface(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_valid_interface", span)?;
    bool_val(valid_interface(&string_arg(args, 0, "nipaddr_valid_interface", span)?))
}

fn nipaddr_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_close", span)?;
    let id = handle_arg(args, 0, "nipaddr_close", span)?;
    let removed = STORE.with(|m| m.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

// ---------------------------------------------------------------------------
// Introspection
// ---------------------------------------------------------------------------

fn nipaddr_kind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_kind", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_kind", span)?, span)?;
    str_val(e.kind())
}

fn nipaddr_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_version", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_version", span)?, span)?;
    int_val(e.version() as i64)
}

fn nipaddr_to_string(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_to_string", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_to_string", span)?, span)?;
    str_val(e.display())
}

fn nipaddr_packed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_packed", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_packed", span)?, span)?;
    bytes_val(&e.packed())
}

fn nipaddr_exploded(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_exploded", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_exploded", span)?, span)?;
    match &e {
        IpEntity::V6Addr(a) => str_val(ipv6_exploded(a)),
        IpEntity::V6Net(n) => str_val(ipv6_exploded(&n.network())),
        IpEntity::V6Iface { addr, .. } => str_val(ipv6_exploded(addr)),
        _ => Err(type_err(span, "exploded() requires IPv6 address or network")),
    }
}

fn nipaddr_compressed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_compressed", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_compressed", span)?, span)?;
    match &e {
        IpEntity::V6Addr(a) => str_val(ipv6_compressed(a)),
        IpEntity::V6Net(n) => str_val(ipv6_compressed(&n.network())),
        IpEntity::V6Iface { addr, .. } => str_val(ipv6_compressed(addr)),
        _ => Err(type_err(span, "compressed() requires IPv6 address or network")),
    }
}

fn nipaddr_reverse_ptr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_reverse_ptr", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_reverse_ptr", span)?, span)?;
    match &e {
        IpEntity::V4Addr(a) => str_val(reverse_ptr_v4(a)),
        IpEntity::V6Addr(a) => str_val(reverse_ptr_v6(a)),
        IpEntity::V4Iface { addr, .. } => str_val(reverse_ptr_v4(addr)),
        IpEntity::V6Iface { addr, .. } => str_val(reverse_ptr_v6(addr)),
        _ => Err(type_err(span, "reverse_ptr() requires an address or interface")),
    }
}

macro_rules! classify_fn {
    ($name:ident, $flat:literal, $short:literal, $v4:expr, $v6:expr) => {
        fn $name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
            arity(args, 1, $flat, span)?;
            let e = get_entity(handle_arg(args, 0, $flat, span)?, span)?;
            let b = match &e {
                IpEntity::V4Addr(a) => $v4(a),
                IpEntity::V6Addr(a) => $v6(a),
                IpEntity::V4Iface { addr, .. } => $v4(addr),
                IpEntity::V6Iface { addr, .. } => $v6(addr),
                _ => {
                    return Err(type_err(
                        span,
                        format!("{}() requires an address or interface", $short),
                    ))
                }
            };
            bool_val(b)
        }
    };
}

classify_fn!(nipaddr_is_private, "nipaddr_is_private", "is_private", v4_is_private, v6_is_private);
classify_fn!(nipaddr_is_global, "nipaddr_is_global", "is_global", v4_is_global, v6_is_global);
classify_fn!(
    nipaddr_is_link_local,
    "nipaddr_is_link_local",
    "is_link_local",
    v4_is_link_local,
    v6_is_link_local
);
classify_fn!(nipaddr_is_loopback, "nipaddr_is_loopback", "is_loopback", v4_is_loopback, v6_is_loopback);
classify_fn!(
    nipaddr_is_multicast,
    "nipaddr_is_multicast",
    "is_multicast",
    v4_is_multicast,
    v6_is_multicast
);
classify_fn!(nipaddr_is_reserved, "nipaddr_is_reserved", "is_reserved", v4_is_reserved, v6_is_reserved);
classify_fn!(
    nipaddr_is_unspecified,
    "nipaddr_is_unspecified",
    "is_unspecified",
    v4_is_unspecified,
    v6_is_unspecified
);

fn nipaddr_is_site_local(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_is_site_local", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_is_site_local", span)?, span)?;
    match &e {
        IpEntity::V6Addr(a) | IpEntity::V6Iface { addr: a, .. } => bool_val(v6_is_site_local(a)),
        _ => bool_val(false),
    }
}

fn nipaddr_max_prefixlen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_max_prefixlen", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_max_prefixlen", span)?, span)?;
    int_val(if e.version() == 4 { 32 } else { 128 })
}

fn nipaddr_add(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_add", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_add", span)?, span)?;
    let delta = match &*args[1].borrow() {
        Value::Int(n) => *n,
        other => {
            return Err(type_err(
                span,
                format!("nipaddr.add() expects int delta, got {}", other.type_name()),
            ))
        }
    };
    match e {
        IpEntity::V4Addr(a) => match add_v4(a, delta) {
            Ok(n) => ok_handle(IpEntity::V4Addr(n)),
            Err(err) => Ok(ip_err(span, err)),
        },
        IpEntity::V6Addr(a) => match add_v6(a, delta as i128) {
            Ok(n) => ok_handle(IpEntity::V6Addr(n)),
            Err(err) => Ok(ip_err(span, err)),
        },
        _ => Err(type_err(span, "add() requires an address")),
    }
}

fn nipaddr_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_compare", span)?;
    let a = get_entity(handle_arg(args, 0, "nipaddr_compare", span)?, span)?;
    let b = get_entity(handle_arg(args, 1, "nipaddr_compare", span)?, span)?;
    use std::cmp::Ordering;
    let ord = match (&a, &b) {
        (IpEntity::V4Addr(x), IpEntity::V4Addr(y)) => compare_v4(x, y),
        (IpEntity::V6Addr(x), IpEntity::V6Addr(y)) => compare_v6(x, y),
        _ => return Err(type_err(span, "compare() requires addresses of the same version")),
    };
    int_val(match ord {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    })
}

// ---------------------------------------------------------------------------
// Network operations
// ---------------------------------------------------------------------------

fn nipaddr_network_address(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_network_address", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_network_address", span)?, span)?;
    match e {
        IpEntity::V4Net(n) => ok_handle(IpEntity::V4Addr(n.network())),
        IpEntity::V6Net(n) => ok_handle(IpEntity::V6Addr(n.network())),
        _ => Err(type_err(span, "network_address() requires a network")),
    }
}

fn nipaddr_broadcast_address(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_broadcast_address", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_broadcast_address", span)?, span)?;
    match e {
        IpEntity::V4Net(n) => ok_handle(IpEntity::V4Addr(broadcast_v4(&n))),
        _ => Err(type_err(span, "broadcast_address() requires an IPv4 network")),
    }
}

fn nipaddr_prefixlen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_prefixlen", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_prefixlen", span)?, span)?;
    match &e {
        IpEntity::V4Net(n) => int_val(n.prefix_len() as i64),
        IpEntity::V6Net(n) => int_val(n.prefix_len() as i64),
        IpEntity::V4Iface { prefix, .. } | IpEntity::V6Iface { prefix, .. } => int_val(*prefix as i64),
        _ => Err(type_err(span, "prefixlen() requires a network or interface")),
    }
}

fn nipaddr_netmask(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_netmask", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_netmask", span)?, span)?;
    match e {
        IpEntity::V4Net(n) => ok_handle(IpEntity::V4Addr(netmask_v4(&n))),
        IpEntity::V4Iface { prefix, .. } => {
            ok_handle(IpEntity::V4Addr(prefix_to_netmask_v4(prefix)))
        }
        _ => Err(type_err(span, "netmask() requires IPv4 network or interface")),
    }
}

fn nipaddr_hostmask(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_hostmask", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_hostmask", span)?, span)?;
    match e {
        IpEntity::V4Net(n) => ok_handle(IpEntity::V4Addr(hostmask_v4(&n))),
        IpEntity::V4Iface { prefix, .. } => {
            ok_handle(IpEntity::V4Addr(prefix_to_hostmask_v4(prefix)))
        }
        _ => Err(type_err(span, "hostmask() requires IPv4 network or interface")),
    }
}

fn nipaddr_num_addresses(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_num_addresses", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_num_addresses", span)?, span)?;
    match e {
        IpEntity::V4Net(n) => {
            let n = num_addresses_v4(&n);
            if n > i64::MAX as u128 {
                Ok(Value::String(n.to_string()).ref_cell())
            } else {
                int_val(n as i64)
            }
        }
        IpEntity::V6Net(n) => Ok(Value::String(num_addresses_v6(&n).to_string()).ref_cell()),
        _ => Err(type_err(span, "num_addresses() requires a network")),
    }
}

fn nipaddr_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_contains", span)?;
    let net = get_entity(handle_arg(args, 0, "nipaddr_contains", span)?, span)?;
    let other = get_entity(handle_arg(args, 1, "nipaddr_contains", span)?, span)?;
    match entity_contains(&net, &other) {
        Ok(b) => bool_val(b),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_overlaps(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_overlaps", span)?;
    let a = get_entity(handle_arg(args, 0, "nipaddr_overlaps", span)?, span)?;
    let b = get_entity(handle_arg(args, 1, "nipaddr_overlaps", span)?, span)?;
    match entity_overlaps(&a, &b) {
        Ok(b) => bool_val(b),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_subnet_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_subnet_of", span)?;
    let a = get_entity(handle_arg(args, 0, "nipaddr_subnet_of", span)?, span)?;
    let b = get_entity(handle_arg(args, 1, "nipaddr_subnet_of", span)?, span)?;
    match subnet_of(&a, &b) {
        Ok(b) => bool_val(b),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_supernet_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_supernet_of", span)?;
    let a = get_entity(handle_arg(args, 0, "nipaddr_supernet_of", span)?, span)?;
    let b = get_entity(handle_arg(args, 1, "nipaddr_supernet_of", span)?, span)?;
    match supernet_of(&a, &b) {
        Ok(b) => bool_val(b),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_hosts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nipaddr_hosts", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_hosts", span)?, span)?;
    let max = optional_int(args, 1, DEFAULT_MAX_HOSTS as i64) as usize;
    let hosts: Vec<IpEntity> = match e {
        IpEntity::V4Net(n) => match collect_hosts_v4(&n, max) {
            Ok(v) => v.into_iter().map(IpEntity::V4Addr).collect(),
            Err(err) => return Ok(ip_err(span, err)),
        },
        IpEntity::V6Net(n) => match collect_hosts_v6(&n, max) {
            Ok(v) => v.into_iter().map(IpEntity::V6Addr).collect(),
            Err(err) => return Ok(ip_err(span, err)),
        },
        _ => return Err(type_err(span, "hosts() requires a network")),
    };
    let ids: Vec<i64> = hosts.into_iter().map(alloc).collect();
    handles_to_array(&ids)
}

fn nipaddr_subnets(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_subnets", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_subnets", span)?, span)?;
    let new_prefix = match &*args[1].borrow() {
        Value::Int(n) => *n as u8,
        other => {
            return Err(type_err(
                span,
                format!("nipaddr.subnets() expects int prefix, got {}", other.type_name()),
            ))
        }
    };
    let nets: Vec<IpEntity> = match e {
        IpEntity::V4Net(n) => match subnets_v4(&n, new_prefix) {
            Ok(v) => v.into_iter().map(IpEntity::V4Net).collect(),
            Err(err) => return Ok(ip_err(span, err)),
        },
        IpEntity::V6Net(n) => match subnets_v6(&n, new_prefix) {
            Ok(v) => v.into_iter().map(IpEntity::V6Net).collect(),
            Err(err) => return Ok(ip_err(span, err)),
        },
        _ => return Err(type_err(span, "subnets() requires a network")),
    };
    let ids: Vec<i64> = nets.into_iter().map(alloc).collect();
    handles_to_array(&ids)
}

fn nipaddr_supernet(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nipaddr_supernet", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_supernet", span)?, span)?;
    let diff = optional_int(args, 1, 1) as u8;
    match e {
        IpEntity::V4Net(n) => match supernet_v4(&n, diff) {
            Ok(s) => ok_handle(IpEntity::V4Net(s)),
            Err(err) => Ok(ip_err(span, err)),
        },
        IpEntity::V6Net(n) => match supernet_v6(&n, diff) {
            Ok(s) => ok_handle(IpEntity::V6Net(s)),
            Err(err) => Ok(ip_err(span, err)),
        },
        _ => Err(type_err(span, "supernet() requires a network")),
    }
}

fn nipaddr_with_prefixlen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_with_prefixlen", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_with_prefixlen", span)?, span)?;
    let prefix = match &*args[1].borrow() {
        Value::Int(n) => *n as u8,
        other => {
            return Err(type_err(
                span,
                format!(
                    "with_prefixlen() expects int prefix, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    match with_prefixlen(&e, prefix) {
        Ok(n) => ok_handle(n),
        Err(err) => Ok(ip_err(span, err)),
    }
}

fn nipaddr_with_netmask(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_with_netmask", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_with_netmask", span)?, span)?;
    let mask_e = get_entity(handle_arg(args, 1, "nipaddr_with_netmask", span)?, span)?;
    let mask = match mask_e {
        IpEntity::V4Addr(a) => a,
        _ => return Err(type_err(span, "with_netmask() mask must be IPv4 address")),
    };
    match with_netmask_v4(&e, mask) {
        Ok(n) => ok_handle(n),
        Err(err) => Ok(ip_err(span, err)),
    }
}

fn nipaddr_with_hostmask(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_with_hostmask", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_with_hostmask", span)?, span)?;
    let mask_e = get_entity(handle_arg(args, 1, "nipaddr_with_hostmask", span)?, span)?;
    let mask = match mask_e {
        IpEntity::V4Addr(a) => a,
        _ => return Err(type_err(span, "with_hostmask() mask must be IPv4 address")),
    };
    match with_hostmask_v4(&e, mask) {
        Ok(n) => ok_handle(n),
        Err(err) => Ok(ip_err(span, err)),
    }
}

fn nipaddr_ip(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_ip", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_ip", span)?, span)?;
    match iface_ip(&e) {
        Ok(a) => ok_handle(a),
        Err(err) => Ok(ip_err(span, err)),
    }
}

fn nipaddr_network_of(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_network_of", span)?;
    let e = get_entity(handle_arg(args, 0, "nipaddr_network_of", span)?, span)?;
    match iface_network(&e) {
        Ok(n) => ok_handle(n),
        Err(err) => Ok(ip_err(span, err)),
    }
}

fn nipaddr_address_exclude(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_address_exclude", span)?;
    let a = get_entity(handle_arg(args, 0, "nipaddr_address_exclude", span)?, span)?;
    let b = get_entity(handle_arg(args, 1, "nipaddr_address_exclude", span)?, span)?;
    let nets: Vec<IpEntity> = match (a, b) {
        (IpEntity::V4Net(x), IpEntity::V4Net(y)) => match address_exclude_v4(&x, &y) {
            Ok(v) => v.into_iter().map(IpEntity::V4Net).collect(),
            Err(err) => return Ok(ip_err(span, err)),
        },
        (IpEntity::V6Net(x), IpEntity::V6Net(y)) => match address_exclude_v6(&x, &y) {
            Ok(v) => v.into_iter().map(IpEntity::V6Net).collect(),
            Err(err) => return Ok(ip_err(span, err)),
        },
        _ => return Err(type_err(span, "address_exclude() requires networks of same version")),
    };
    let ids: Vec<i64> = nets.into_iter().map(alloc).collect();
    handles_to_array(&ids)
}

fn nipaddr_summarize_range(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_summarize_range", span)?;
    let first = get_entity(handle_arg(args, 0, "nipaddr_summarize_range", span)?, span)?;
    let last = get_entity(handle_arg(args, 1, "nipaddr_summarize_range", span)?, span)?;
    match summarize_range(&first, &last) {
        Ok(nets) => {
            let ids: Vec<i64> = nets.into_iter().map(alloc).collect();
            handles_to_array(&ids)
        }
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_collapse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nipaddr_collapse", span)?;
    let ids = handles_from_array(args, 0, "nipaddr_collapse", span)?;
    let entities = entities_from_handles(&ids, span)?;
    match collapse_networks(&entities) {
        Ok(nets) => {
            let out: Vec<i64> = nets.into_iter().map(alloc).collect();
            handles_to_array(&out)
        }
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_contains_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_contains_many", span)?;
    let net = get_entity(handle_arg(args, 0, "nipaddr_contains_many", span)?, span)?;
    let ids = handles_from_array(args, 1, "nipaddr_contains_many", span)?;
    let candidates = entities_from_handles(&ids, span)?;
    match contains_many(&net, &candidates) {
        Ok(flags) => Ok(Value::BoolArray(flags.into_iter().map(|b| b as u8)).ref_cell()),
        Err(e) => Ok(ip_err(span, e)),
    }
}

fn nipaddr_filter_contains(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nipaddr_filter_contains", span)?;
    let net = get_entity(handle_arg(args, 0, "nipaddr_filter_contains", span)?, span)?;
    let ids = handles_from_array(args, 1, "nipaddr_filter_contains", span)?;
    let candidates = entities_from_handles(&ids, span)?;
    match filter_containing(&net, &candidates) {
        Ok(matched) => {
            let out: Vec<i64> = matched.into_iter().map(alloc).collect();
            handles_to_array(&out)
        }
        Err(e) => Ok(ip_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nipaddr_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nipaddr_fns![
    ("nipaddr_address", "address", nipaddr_address),
    ("nipaddr_ipv4", "ipv4", nipaddr_ipv4),
    ("nipaddr_ipv6", "ipv6", nipaddr_ipv6),
    ("nipaddr_network", "network", nipaddr_network),
    ("nipaddr_ipv4_network", "ipv4_network", nipaddr_ipv4_network),
    ("nipaddr_ipv6_network", "ipv6_network", nipaddr_ipv6_network),
    ("nipaddr_interface", "interface", nipaddr_interface),
    ("nipaddr_valid_address", "valid_address", nipaddr_valid_address),
    ("nipaddr_valid_network", "valid_network", nipaddr_valid_network),
    ("nipaddr_valid_interface", "valid_interface", nipaddr_valid_interface),
    ("nipaddr_close", "close", nipaddr_close),
    ("nipaddr_kind", "kind", nipaddr_kind),
    ("nipaddr_version", "version", nipaddr_version),
    ("nipaddr_to_string", "to_string", nipaddr_to_string),
    ("nipaddr_packed", "packed", nipaddr_packed),
    ("nipaddr_exploded", "exploded", nipaddr_exploded),
    ("nipaddr_compressed", "compressed", nipaddr_compressed),
    ("nipaddr_reverse_ptr", "reverse_ptr", nipaddr_reverse_ptr),
    ("nipaddr_is_private", "is_private", nipaddr_is_private),
    ("nipaddr_is_global", "is_global", nipaddr_is_global),
    ("nipaddr_is_link_local", "is_link_local", nipaddr_is_link_local),
    ("nipaddr_is_loopback", "is_loopback", nipaddr_is_loopback),
    ("nipaddr_is_multicast", "is_multicast", nipaddr_is_multicast),
    ("nipaddr_is_reserved", "is_reserved", nipaddr_is_reserved),
    ("nipaddr_is_unspecified", "is_unspecified", nipaddr_is_unspecified),
    ("nipaddr_is_site_local", "is_site_local", nipaddr_is_site_local),
    ("nipaddr_max_prefixlen", "max_prefixlen", nipaddr_max_prefixlen),
    ("nipaddr_add", "add", nipaddr_add),
    ("nipaddr_compare", "compare", nipaddr_compare),
    ("nipaddr_network_address", "network_address", nipaddr_network_address),
    ("nipaddr_broadcast_address", "broadcast_address", nipaddr_broadcast_address),
    ("nipaddr_prefixlen", "prefixlen", nipaddr_prefixlen),
    ("nipaddr_netmask", "netmask", nipaddr_netmask),
    ("nipaddr_hostmask", "hostmask", nipaddr_hostmask),
    ("nipaddr_num_addresses", "num_addresses", nipaddr_num_addresses),
    ("nipaddr_contains", "contains", nipaddr_contains),
    ("nipaddr_overlaps", "overlaps", nipaddr_overlaps),
    ("nipaddr_subnet_of", "subnet_of", nipaddr_subnet_of),
    ("nipaddr_supernet_of", "supernet_of", nipaddr_supernet_of),
    ("nipaddr_hosts", "hosts", nipaddr_hosts),
    ("nipaddr_subnets", "subnets", nipaddr_subnets),
    ("nipaddr_supernet", "supernet", nipaddr_supernet),
    ("nipaddr_with_prefixlen", "with_prefixlen", nipaddr_with_prefixlen),
    ("nipaddr_with_netmask", "with_netmask", nipaddr_with_netmask),
    ("nipaddr_with_hostmask", "with_hostmask", nipaddr_with_hostmask),
    ("nipaddr_ip", "ip", nipaddr_ip),
    ("nipaddr_network_of", "network_of", nipaddr_network_of),
    ("nipaddr_address_exclude", "address_exclude", nipaddr_address_exclude),
    ("nipaddr_summarize_range", "summarize_range", nipaddr_summarize_range),
    ("nipaddr_collapse", "collapse", nipaddr_collapse),
    ("nipaddr_contains_many", "contains_many", nipaddr_contains_many),
    ("nipaddr_filter_contains", "filter_contains", nipaddr_filter_contains),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nipaddr";
pub const MODULE_PATHS: &[&str] = &["nipaddr", "std/nipaddr"];

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
    fn address_doctest() {
        let h = nipaddr_address(&[Value::String("192.168.1.1".into()).ref_cell()], span()).unwrap();
        let s = nipaddr_to_string(&[h], span()).unwrap();
        assert_eq!(*s.borrow(), Value::String("192.168.1.1".into()));
    }

    #[test]
    fn contains_doctest() {
        let net = nipaddr_network(
            &[Value::String("10.0.0.0/8".into()).ref_cell()],
            span(),
        )
        .unwrap();
        let addr = nipaddr_address(&[Value::String("10.1.2.3".into()).ref_cell()], span()).unwrap();
        let ok = nipaddr_contains(&[net, addr], span()).unwrap();
        assert_eq!(*ok.borrow(), Value::Bool(true));
    }
}
