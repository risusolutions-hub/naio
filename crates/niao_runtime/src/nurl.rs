//! Native nurl standard library — URL parse, build, join, query helpers,
//! and RFC 3986 percent encoding. Hand-rolled (std only).
//!
//! Import with `import "nurl"` (or `import "std/nurl"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

// Error codes (wired into codes.rs by central integration).
const E_NURL_ARITY: u32 = 2880;
const E_NURL_ERROR: u32 = 2881;
const E_NURL_TYPE: u32 = 2882;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E_NURL_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::at(
            span,
            E_NURL_TYPE,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        other => Err(RuntimeError::at(
            span,
            E_NURL_TYPE,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn nurl_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E_NURL_ERROR, "nurl_error", msg.into(), span)
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

fn opt_string_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn opt_int_field(map: &HashMap<String, ValueRef>, key: &str) -> Option<i64> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n),
        Value::Nil => None,
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Percent encoding (RFC 3986)
// ---------------------------------------------------------------------------

fn hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'A' + n - 10) as char,
    }
}

fn from_hex(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("bad hex".into()),
    }
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex(b >> 4));
            out.push(hex(b & 0xf));
        }
    }
    out
}

fn percent_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("bad percent escape".into());
            }
            let hi = from_hex(bytes[i + 1])?;
            let lo = from_hex(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "invalid utf8".into())
}

/// Query-string decode: percent escapes and `+` as space.
fn query_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("bad percent escape".into());
            }
            let hi = from_hex(bytes[i + 1])?;
            let lo = from_hex(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "invalid utf8".into())
}

// ---------------------------------------------------------------------------
// URL parse / build
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct UrlParts {
    scheme: String,
    host: String,
    port: u16,
    path: String,
    query: String,
    fragment: String,
    user: String,
    password: String,
}

fn default_port(scheme: &str) -> u16 {
    match scheme {
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        "ftp" => 21,
        _ => 0,
    }
}

fn trim_ascii_whitespace(s: &str) -> &str {
    s.trim_matches(|c: char| matches!(c, '\u{0009}' | '\u{000A}' | '\u{000D}' | ' '))
}

fn split_once(s: &str, ch: char) -> (&str, String) {
    match s.split_once(ch) {
        Some((a, b)) => (a, b.to_string()),
        None => (s, String::new()),
    }
}

fn validate_scheme(scheme: &str) -> Result<(), String> {
    if scheme.is_empty() {
        return Err("missing scheme".into());
    }
    let first = scheme.chars().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err("invalid scheme".into());
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return Err("invalid scheme".into());
    }
    Ok(())
}

fn parse_authority(auth: &str) -> Result<(String, String, &str), String> {
    if auth.is_empty() {
        return Err("missing authority".into());
    }
    if let Some(at) = auth.rfind('@') {
        let creds = &auth[..at];
        let hostport = &auth[at + 1..];
        if hostport.is_empty() {
            return Err("missing host".into());
        }
        let (user, pass) = match creds.split_once(':') {
            Some((u, p)) => (query_decode(u)?, query_decode(p)?),
            None => (query_decode(creds)?, String::new()),
        };
        Ok((user, pass, hostport))
    } else {
        Ok((String::new(), String::new(), auth))
    }
}

fn parse_hostport(hostport: &str, scheme: &str) -> Result<(String, u16), String> {
    if hostport.is_empty() {
        return Err("missing host".into());
    }
    if hostport.starts_with('[') {
        let end = hostport.find(']').ok_or("bad ipv6 host")?;
        let host = hostport[1..end].to_string();
        if host.is_empty() {
            return Err("bad ipv6 host".into());
        }
        let port = if hostport.len() > end + 1 {
            if hostport.as_bytes().get(end + 1).copied() != Some(b':') {
                return Err("bad ipv6 port".into());
            }
            let p: u16 = hostport[end + 2..]
                .parse()
                .map_err(|_| "bad port".to_string())?;
            if p == 0 {
                return Err("bad port".into());
            }
            p
        } else {
            default_port(scheme)
        };
        return Ok((host, port));
    }
    if let Some(colon) = hostport.rfind(':') {
        let host = &hostport[..colon];
        if host.is_empty() {
            return Err("missing host".into());
        }
        let port: u16 = hostport[colon + 1..]
            .parse()
            .map_err(|_| "bad port".to_string())?;
        if port == 0 {
            return Err("bad port".into());
        }
        Ok((host.to_string(), port))
    } else {
        Ok((hostport.to_string(), default_port(scheme)))
    }
}

fn parse_url(raw: &str) -> Result<UrlParts, String> {
    let raw = trim_ascii_whitespace(raw);
    if raw.is_empty() {
        return Err("empty url".into());
    }

    let (raw, fragment) = split_once(raw, '#');
    let (raw, query) = split_once(raw, '?');

    let scheme_end = raw.find(':').ok_or("missing scheme")?;
    if scheme_end == 0 {
        return Err("missing scheme".into());
    }
    let scheme_raw = &raw[..scheme_end];
    validate_scheme(scheme_raw)?;
    let scheme = scheme_raw.to_ascii_lowercase();

    let rest = &raw[scheme_end + 1..];
    if !rest.starts_with("//") {
        return Err("missing authority".into());
    }
    let rest = &rest[2..];
    let (auth, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let (user, password, hostport) = parse_authority(auth)?;
    let (host, port) = parse_hostport(hostport, &scheme)?;

    let path = if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    };

    Ok(UrlParts {
        scheme,
        host,
        port,
        path,
        query,
        fragment,
        user,
        password,
    })
}

fn userinfo_string(url: &UrlParts) -> Option<String> {
    if url.user.is_empty() {
        return None;
    }
    if url.password.is_empty() {
        Some(url.user.clone())
    } else {
        Some(format!("{}:{}", url.user, url.password))
    }
}

fn parse_userinfo(s: &str) -> (String, String) {
    if s.is_empty() {
        return (String::new(), String::new());
    }
    match s.split_once(':') {
        Some((u, p)) => (u.to_string(), p.to_string()),
        None => (s.to_string(), String::new()),
    }
}

fn url_to_string(url: &UrlParts) -> String {
    let mut s = String::with_capacity(
        url.scheme.len()
            + url.host.len()
            + url.path.len()
            + url.query.len()
            + url.fragment.len()
            + 16,
    );
    s.push_str(&url.scheme);
    s.push_str("://");
    if !url.user.is_empty() {
        s.push_str(&percent_encode(url.user.as_bytes()));
        if !url.password.is_empty() {
            s.push(':');
            s.push_str(&percent_encode(url.password.as_bytes()));
        }
        s.push('@');
    }
    if url.host.contains(':') {
        s.push('[');
        s.push_str(&url.host);
        s.push(']');
    } else {
        s.push_str(&url.host);
    }
    let def = default_port(&url.scheme);
    if url.port != 0 && url.port != def {
        s.push(':');
        s.push_str(&url.port.to_string());
    }
    if url.path.is_empty() {
        s.push('/');
    } else {
        s.push_str(&url.path);
    }
    if !url.query.is_empty() {
        s.push('?');
        s.push_str(&url.query);
    }
    if !url.fragment.is_empty() {
        s.push('#');
        s.push_str(&url.fragment);
    }
    s
}

fn parts_to_object(url: &UrlParts) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    map.insert("scheme".to_string(), Value::String(url.scheme.clone()).ref_cell());
    map.insert("host".to_string(), Value::String(url.host.clone()).ref_cell());
    map.insert("path".to_string(), Value::String(url.path.clone()).ref_cell());

    let def = default_port(&url.scheme);
    if url.port != 0 && url.port != def {
        map.insert("port".to_string(), Value::Int(url.port as i64).ref_cell());
    } else {
        map.insert("port".to_string(), Value::Nil.ref_cell());
    }

    if url.query.is_empty() {
        map.insert("query".to_string(), Value::Nil.ref_cell());
    } else {
        map.insert(
            "query".to_string(),
            Value::String(url.query.clone()).ref_cell(),
        );
    }

    if url.fragment.is_empty() {
        map.insert("fragment".to_string(), Value::Nil.ref_cell());
    } else {
        map.insert(
            "fragment".to_string(),
            Value::String(url.fragment.clone()).ref_cell(),
        );
    }

    if let Some(info) = userinfo_string(url) {
        map.insert("userinfo".to_string(), Value::String(info).ref_cell());
    } else {
        map.insert("userinfo".to_string(), Value::Nil.ref_cell());
    }

    map
}

fn parts_from_object(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<UrlParts> {
    let scheme = opt_string_field(map, "scheme").ok_or_else(|| {
        RuntimeError::at(span, E_NURL_TYPE, "nurl_build() requires scheme string")
    })?;
    let host = opt_string_field(map, "host").ok_or_else(|| {
        RuntimeError::at(span, E_NURL_TYPE, "nurl_build() requires host string")
    })?;

    let path = opt_string_field(map, "path").unwrap_or_else(|| "/".to_string());
    let query = opt_string_field(map, "query").unwrap_or_default();
    let fragment = opt_string_field(map, "fragment").unwrap_or_default();

    let port = match opt_int_field(map, "port") {
        Some(p) if p > 0 && p <= u16::MAX as i64 => p as u16,
        Some(_) => {
            return Err(RuntimeError::at(
                span,
                E_NURL_TYPE,
                "nurl_build() port must be 1..=65535",
            ))
        }
        None => default_port(&scheme),
    };

    let (user, password) = match opt_string_field(map, "userinfo") {
        Some(info) => parse_userinfo(&info),
        None => (String::new(), String::new()),
    };

    Ok(UrlParts {
        scheme,
        host,
        port,
        path,
        query,
        fragment,
        user,
        password,
    })
}

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

fn parse_query_pairs(query: &str) -> Result<Vec<(String, String)>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut pairs = Vec::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some(eq) = pair.find('=') {
            let key = query_decode(&pair[..eq])?;
            let val = query_decode(&pair[eq + 1..])?;
            pairs.push((key, val));
        } else {
            pairs.push((query_decode(pair)?, String::new()));
        }
    }
    Ok(pairs)
}

fn query_pairs_to_object(pairs: &[(String, String)]) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    for (k, v) in pairs {
        map.insert(k.clone(), Value::String(v.clone()).ref_cell());
    }
    map
}

fn build_query_from_object(obj: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<String> {
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    let mut parts = Vec::new();
    for key in keys {
        let val = &*obj[key].borrow();
        let encoded_key = percent_encode(key.as_bytes());
        let encoded_val = match val {
            Value::String(s) => percent_encode(s.as_bytes()),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Nil => String::new(),
            other => {
                return Err(RuntimeError::at(
                    span,
                    E_NURL_TYPE,
                    format!(
                        "nurl_set_query() query values must be strings or numbers, got {}",
                        other.type_name()
                    ),
                ));
            }
        };
        if encoded_val.is_empty() {
            parts.push(encoded_key);
        } else {
            parts.push(format!("{encoded_key}={encoded_val}"));
        }
    }
    Ok(parts.join("&"))
}

// ---------------------------------------------------------------------------
// URL join (RFC 3986)
// ---------------------------------------------------------------------------

fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if absolute {
                    segments.pop();
                } else if !segments.is_empty() {
                    segments.pop();
                }
            }
            other => segments.push(other),
        }
    }
    if absolute {
        if segments.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", segments.join("/"))
        }
    } else if segments.is_empty() {
        String::new()
    } else {
        segments.join("/")
    }
}

fn merge_paths(base_path: &str, reference: &str) -> String {
    if reference.starts_with('/') {
        return normalize_path(reference);
    }
    let base_dir = base_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let merged = if base_dir.is_empty() {
        reference.to_string()
    } else {
        format!("{base_dir}/{reference}")
    };
    normalize_path(&format!("/{merged}"))
        .trim_start_matches('/')
        .to_string()
}

fn join_url(base: &UrlParts, reference: &str) -> Result<UrlParts, String> {
    let reference = trim_ascii_whitespace(reference);
    if reference.is_empty() {
        return Ok(base.clone());
    }

    if let Some(scheme_end) = reference.find(':') {
        if scheme_end > 0
            && reference[..scheme_end]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            && reference[scheme_end + 1..].starts_with("//")
        {
            return parse_url(reference);
        }
    }

    if reference.starts_with("//") {
        return parse_url(&format!("{}:{}", base.scheme, reference));
    }

    let mut out = base.clone();

    if reference.starts_with('#') {
        out.fragment = reference[1..].to_string();
        return Ok(out);
    }

    let (ref_path_query, fragment) = match reference.find('#') {
        Some(i) => (&reference[..i], reference[i + 1..].to_string()),
        None => (reference, String::new()),
    };

    let (ref_path, query) = match ref_path_query.find('?') {
        Some(i) => (&ref_path_query[..i], ref_path_query[i + 1..].to_string()),
        None => (ref_path_query, String::new()),
    };

    if ref_path.starts_with('?') {
        out.query = ref_path[1..].to_string();
        out.fragment = fragment;
        return Ok(out);
    }

    if !ref_path.is_empty() {
        if ref_path.starts_with('/') {
            out.path = normalize_path(ref_path);
        } else {
            let merged = merge_paths(&base.path, ref_path);
            out.path = if merged.is_empty() {
                "/".to_string()
            } else if merged.starts_with('/') {
                merged
            } else {
                format!("/{merged}")
            };
        }
        if !query.is_empty() {
            out.query = query;
        } else if !ref_path_query.contains('?') {
            out.query.clear();
        }
    } else if !query.is_empty() {
        out.query = query;
    }

    out.fragment = fragment;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nurl_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nurl_parse", span)?;
    let raw = string_arg(args, 0, "nurl_parse", span)?;
    match parse_url(&raw) {
        Ok(url) => Ok(Value::Object(parts_to_object(&url)).ref_cell()),
        Err(e) => Ok(nurl_error(span, e)),
    }
}

fn nurl_build(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nurl_build", span)?;
    let map = object_arg(args, 0, "nurl_build", span)?;
    match parts_from_object(&map, span) {
        Ok(parts) => str_val(url_to_string(&parts)),
        Err(e) => Err(e),
    }
}

fn nurl_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nurl_encode", span)?;
    let s = string_arg(args, 0, "nurl_encode", span)?;
    str_val(percent_encode(s.as_bytes()))
}

fn nurl_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nurl_decode", span)?;
    let s = string_arg(args, 0, "nurl_decode", span)?;
    match percent_decode(&s) {
        Ok(decoded) => str_val(decoded),
        Err(e) => Ok(nurl_error(span, e)),
    }
}

fn nurl_set_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nurl_set_query", span)?;
    let query_obj = object_arg(args, 1, "nurl_set_query", span)?;
    let query = build_query_from_object(&query_obj, span)?;

    match &*args[0].borrow() {
        Value::String(s) => match parse_url(s) {
            Ok(mut url) => {
                url.query = query;
                str_val(url_to_string(&url))
            }
            Err(e) => Ok(nurl_error(span, e)),
        },
        Value::Object(map) => {
            let mut parts = parts_from_object(map, span)?;
            parts.query = query;
            str_val(url_to_string(&parts))
        }
        other => Err(RuntimeError::at(
            span,
            E_NURL_TYPE,
            format!(
                "nurl_set_query() expects url string or parts object as argument 1, got {}",
                other.type_name()
            ),
        )),
    }
}

fn nurl_get_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nurl_get_query", span)?;
    let raw = string_arg(args, 0, "nurl_get_query", span)?;
    match parse_url(&raw) {
        Ok(url) => match parse_query_pairs(&url.query) {
            Ok(pairs) => Ok(Value::Object(query_pairs_to_object(&pairs)).ref_cell()),
            Err(e) => Ok(nurl_error(span, e)),
        },
        Err(e) => Ok(nurl_error(span, e)),
    }
}

fn nurl_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nurl_join", span)?;
    let base = string_arg(args, 0, "nurl_join", span)?;
    let reference = string_arg(args, 1, "nurl_join", span)?;
    match parse_url(&base).and_then(|b| join_url(&b, &reference)) {
        Ok(url) => str_val(url_to_string(&url)),
        Err(e) => Ok(nurl_error(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nurl_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nurl_fns![
    ("nurl_parse", "parse", nurl_parse),
    ("nurl_build", "build", nurl_build),
    ("nurl_encode", "encode", nurl_encode),
    ("nurl_decode", "decode", nurl_decode),
    ("nurl_set_query", "set_query", nurl_set_query),
    ("nurl_get_query", "get_query", nurl_get_query),
    ("nurl_join", "join", nurl_join),
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

pub const MODULE_NAME: &str = "nurl";
pub const MODULE_PATHS: &[&str] = &["nurl", "std/nurl"];

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

    fn expect_str(v: NiaoResult<ValueRef>) -> String {
        match &*v.unwrap().borrow() {
            Value::String(s) => s.clone(),
            other => panic!("expected string, got {other:?}"),
        }
    }

    fn expect_object(v: NiaoResult<ValueRef>) -> HashMap<String, ValueRef> {
        match &*v.unwrap().borrow() {
            Value::Object(map) => map.clone(),
            other => panic!("expected object, got {other:?}"),
        }
    }

    #[test]
    fn ipv6_roundtrip() {
        let map = expect_object(nurl_parse(
            &[Value::String("http://[::1]:8080/".into()).ref_cell()],
            span(),
        ));
        assert!(matches!(
            &*map["host"].borrow(),
            Value::String(s) if s == "::1"
        ));
        let built = expect_str(nurl_build(&[Value::Object(map).ref_cell()], span()));
        assert_eq!(built, "http://[::1]:8080/");
    }

    #[test]
    fn parse_https() {
        let map = expect_object(nurl_parse(
            &[Value::String("https://user:pass@example.com:8443/path?q=1#frag".into()).ref_cell()],
            span(),
        ));
        assert!(matches!(
            &*map["scheme"].borrow(),
            Value::String(s) if s == "https"
        ));
        assert!(matches!(
            &*map["host"].borrow(),
            Value::String(s) if s == "example.com"
        ));
        assert!(matches!(&*map["port"].borrow(), Value::Int(8443)));
        assert!(matches!(
            &*map["path"].borrow(),
            Value::String(s) if s == "/path"
        ));
        assert!(matches!(
            &*map["query"].borrow(),
            Value::String(s) if s == "q=1"
        ));
        assert!(matches!(
            &*map["fragment"].borrow(),
            Value::String(s) if s == "frag"
        ));
        assert!(matches!(
            &*map["userinfo"].borrow(),
            Value::String(s) if s == "user:pass"
        ));
    }

    #[test]
    fn parse_default_port_nil() {
        let map = expect_object(nurl_parse(
            &[Value::String("http://example.com/".into()).ref_cell()],
            span(),
        ));
        assert!(matches!(&*map["port"].borrow(), Value::Nil));
        assert!(matches!(&*map["query"].borrow(), Value::Nil));
        assert!(matches!(&*map["fragment"].borrow(), Value::Nil));
        assert!(matches!(&*map["userinfo"].borrow(), Value::Nil));
    }

    #[test]
    fn encode_decode_roundtrip() {
        let s = "hello world!@#";
        let enc = expect_str(nurl_encode(
            &[Value::String(s.into()).ref_cell()],
            span(),
        ));
        assert_eq!(enc, "hello%20world%21%40%23");
        let dec = expect_str(nurl_decode(&[Value::String(enc).ref_cell()], span()));
        assert_eq!(dec, s);
    }

    #[test]
    fn build_and_join() {
        let mut parts = HashMap::new();
        parts.insert("scheme".to_string(), Value::String("https".into()).ref_cell());
        parts.insert("host".to_string(), Value::String("example.com".into()).ref_cell());
        parts.insert("path".to_string(), Value::String("/a/b/".into()).ref_cell());
        let built = expect_str(nurl_build(&[Value::Object(parts).ref_cell()], span()));
        assert_eq!(built, "https://example.com/a/b/");

        let joined = expect_str(nurl_join(
            &[
                Value::String("https://example.com/a/b/".into()).ref_cell(),
                Value::String("c".into()).ref_cell(),
            ],
            span(),
        ));
        assert_eq!(joined, "https://example.com/a/b/c");
    }

    #[test]
    fn query_get_set() {
        let q = expect_object(nurl_get_query(
            &[Value::String("http://h/?a=1&b=hello%20world".into()).ref_cell()],
            span(),
        ));
        assert!(matches!(&*q["a"].borrow(), Value::String(s) if s == "1"));
        assert!(matches!(
            &*q["b"].borrow(),
            Value::String(s) if s == "hello world"
        ));

        let mut params = HashMap::new();
        params.insert("x".to_string(), Value::String("a b".into()).ref_cell());
        params.insert("y".to_string(), Value::Int(42).ref_cell());
        let url = expect_str(nurl_set_query(
            &[
                Value::String("https://ex.com/path".into()).ref_cell(),
                Value::Object(params).ref_cell(),
            ],
            span(),
        ));
        assert!(url.contains("x=a%20b"));
        assert!(url.contains("y=42"));
    }

    #[test]
    fn join_relative_paths() {
        let joined = expect_str(nurl_join(
            &[
                Value::String("http://a/b/c/d;p?q".into()).ref_cell(),
                Value::String("../g".into()).ref_cell(),
            ],
            span(),
        ));
        assert_eq!(joined, "http://a/b/g");
    }
}
