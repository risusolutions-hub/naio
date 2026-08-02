//! URL parsing and encoding utilities via `niao_http`.

use super::{net_error, ok_string, string_arg, NetResult};
use niao_ast::Span;
use niao_errors::codes;
use niao_http::{form_urlencode, join, parse_url, percent_decode, Url};
use std::collections::HashMap;

pub fn net_url_parse(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 1, "net_url_parse", span)?;
    let raw = string_arg(args, 0, "net_url_parse", span)?;
    match parse_url(&raw) {
        Ok(u) => {
            let mut map = HashMap::new();
            map.insert("scheme".into(), ok_string(u.scheme));
            map.insert("host".into(), ok_string(u.host));
            map.insert("port".into(), crate::Value::Int(u.port as i64).ref_cell());
            map.insert("path".into(), ok_string(u.path));
            map.insert("query".into(), ok_string(u.query));
            map.insert("fragment".into(), ok_string(u.fragment));
            map.insert("user".into(), ok_string(u.user));
            map.insert("password".into(), ok_string(u.password));
            Ok(crate::Value::Object(map).ref_cell())
        }
        Err(e) => Ok(net_error(span, codes::E1403_NET_URL, "net_url_error", e)),
    }
}

pub fn net_url_encode(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 1, "net_url_encode", span)?;
    let s = string_arg(args, 0, "net_url_encode", span)?;
    Ok(ok_string(form_urlencode(s.as_bytes())))
}

pub fn net_url_decode(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 1, "net_url_decode", span)?;
    let s = string_arg(args, 0, "net_url_decode", span)?;
    match percent_decode(&s) {
        Ok(decoded) => Ok(ok_string(decoded)),
        Err(e) => Ok(net_error(span, codes::E1403_NET_URL, "net_url_error", e)),
    }
}

pub fn net_url_join(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 2, "net_url_join", span)?;
    let base = string_arg(args, 0, "net_url_join", span)?;
    let reference = string_arg(args, 1, "net_url_join", span)?;
    match parse_url(&base).and_then(|b| join(&b, &reference)) {
        Ok(u) => Ok(ok_string(u.to_string_full())),
        Err(e) => Ok(net_error(span, codes::E1403_NET_URL, "net_url_error", e)),
    }
}

pub fn net_url_build(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity(args, 1, "net_url_build", span)?;
    let parts = super::object_arg(args, 0, "net_url_build", span)?;
    let scheme = super::object_string_field(&parts, "scheme", span)?;
    let host = super::object_string_field(&parts, "host", span)?;
    let path = super::object_string_field(&parts, "path", span).unwrap_or_else(|_| "/".into());
    let query = super::object_string_field(&parts, "query", span).unwrap_or_default();
    let fragment = super::object_string_field(&parts, "fragment", span).unwrap_or_default();
    let port = super::object_int_field(&parts, "port", span).ok();

    let url = Url {
        scheme,
        host,
        port: port.filter(|&p| p > 0).unwrap_or(0) as u16,
        path,
        query,
        fragment,
        user: String::new(),
        password: String::new(),
    };
    let mut built = url.to_string_full();
    if port.is_some() && port.unwrap() > 0 {
        // to_string_full already handles port when non-default
    }
    match parse_url(&built) {
        Ok(u) => Ok(ok_string(u.to_string_full())),
        Err(e) => Ok(net_error(span, codes::E1403_NET_URL, "net_url_error", e)),
    }
}
