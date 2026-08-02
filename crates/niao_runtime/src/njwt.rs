//! Native njwt standard library — JWT / JWS sign + verify (HS/RS/ES/EdDSA),
//! claims validation, JWKS fetch. ~PyJWT / python-jose subset.
//!
//! Import with `import "njwt"` (or `import "std/njwt"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_jwt::{
    decode_unverified, fetch_jwks, header, parse_jwks, sign, valid, verify, verify_all, verify_jwks,
    FetchOptions, Jwks, Key, SignOptions, VerifyOptions, SUPPORTED,
};
use niao_json_core::{to_string, Number as JsonNumber, Value as JsonValue};
use niao_parallel::available_threads;
use std::collections::HashMap;
use std::rc::Rc;

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4432_NJWT_TYPE, msg.into())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E4430_NJWT_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn njwt_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E4431_NJWT_ERROR, "njwt_error", msg.into(), span)
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

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return None;
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Some(map.clone()),
        Value::Nil => None,
        _ => None,
    }
}

fn bool_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    match map.and_then(|m| m.get(key).map(|v| v.borrow().clone())) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str, default: i64) -> i64 {
    match map.and_then(|m| m.get(key).map(|v| v.borrow().clone())) {
        Some(Value::Int(n)) => n,
        _ => default,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    match map.and_then(|m| m.get(key).map(|v| v.borrow().clone())) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn claims_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<JsonValue> {
    runtime_to_niao(&*args[idx].borrow(), span, name)
}

fn runtime_to_niao(v: &Value, span: Span, name: &str) -> NiaoResult<JsonValue> {
    match v {
        Value::Nil => Ok(JsonValue::Null),
        Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        Value::Int(n) => Ok(JsonValue::Number(JsonNumber::I64(*n))),
        Value::Float(f) => Ok(JsonValue::Number(JsonNumber::F64(*f))),
        Value::String(s) => Ok(JsonValue::String(s.clone())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(runtime_to_niao(&*item.borrow(), span, name)?);
            }
            Ok(JsonValue::Array(out))
        }
        Value::Object(map) => {
            let mut out = niao_json_core::Object::new();
            for (k, vr) in map {
                out.insert(k.clone(), runtime_to_niao(&*vr.borrow(), span, name)?);
            }
            Ok(JsonValue::Object(out))
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects JSON-serializable value, got {}",
                other.type_name()
            ),
        )),
    }
}

fn niao_to_runtime(v: JsonValue) -> ValueRef {
    match v {
        JsonValue::Null => Value::Nil.ref_cell(),
        JsonValue::Bool(b) => Value::Bool(b).ref_cell(),
        JsonValue::Number(n) => match n {
            JsonNumber::I64(i) => Value::Int(i).ref_cell(),
            JsonNumber::U64(u) if u <= i64::MAX as u64 => Value::Int(u as i64).ref_cell(),
            JsonNumber::U64(u) => Value::String(u.to_string()).ref_cell(),
            JsonNumber::F64(f) => Value::Float(f).ref_cell(),
        },
        JsonValue::String(s) => Value::String(s).ref_cell(),
        JsonValue::Array(items) => {
            let out: Vec<ValueRef> = items.into_iter().map(niao_to_runtime).collect();
            Value::Array(out).ref_cell()
        }
        JsonValue::Object(map) => {
            let mut out = HashMap::new();
            for (k, v) in map {
                out.insert(k, niao_to_runtime(v));
            }
            Value::Object(out).ref_cell()
        }
    }
}

fn parse_key_arg(v: &Value, span: Span) -> Result<Key, ValueRef> {
    match v {
        Value::String(s) => {
            if s.trim_start().starts_with("-----BEGIN") {
                Key::from_pem(s, None).map_err(|e| njwt_err(span, e.message()))
            } else {
                Key::from_secret(s.as_bytes(), None).map_err(|e| njwt_err(span, e.message()))
            }
        }
        Value::ByteArray(b) => {
            Key::from_secret(b, None).map_err(|e| njwt_err(span, e.message()))
        }
        Value::Object(map) => {
            let alg = map
                .get("alg")
                .and_then(|v| match &*v.borrow() {
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                });
            if let Some(pem_v) = map.get("pem") {
                let pem = match &*pem_v.borrow() {
                    Value::String(s) => s.clone(),
                    other => return Err(njwt_err(span, format!("key.pem must be string, got {}", other.type_name()))),
                };
                return Key::from_pem(&pem, alg).map_err(|e| njwt_err(span, e.message()));
            }
            if let Some(sec_v) = map.get("secret") {
                let secret = match &*sec_v.borrow() {
                    Value::String(s) => s.as_bytes().to_vec(),
                    Value::ByteArray(b) => b.clone(),
                    other => {
                        return Err(njwt_err(
                            span,
                            format!("key.secret must be string or bytes, got {}", other.type_name()),
                        ));
                    }
                };
                return Key::from_secret(&secret, alg).map_err(|e| njwt_err(span, e.message()));
            }
            Err(njwt_err(span, "key object requires 'secret' or 'pem'"))
        }
        other => Err(njwt_err(
            span,
            format!("key must be string, bytes, or object — got {}", other.type_name()),
        )),
    }
}

fn sign_opts_from(map: Option<&HashMap<String, ValueRef>>) -> SignOptions {
    let mut opts = SignOptions::default();
    if let Some(alg) = string_field(map, "alg") {
        opts.alg = alg;
    }
    opts.kid = string_field(map, "kid");
    if let Some(typ) = string_field(map, "typ") {
        opts.typ = Some(typ);
    }
    opts
}

fn verify_opts_from(map: Option<&HashMap<String, ValueRef>>) -> VerifyOptions {
    let mut opts = VerifyOptions::default();
    if let Some(m) = map {
        if let Some(Value::Array(items)) = m.get("algorithms").map(|v| v.borrow().clone()) {
            opts.algorithms = items
                .into_iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s),
                    _ => None,
                })
                .collect();
        } else if let Some(alg) = string_field(map, "alg") {
            opts.algorithms = vec![alg];
        }
    }
    opts.validate_exp = bool_field(map, "validate_exp", true);
    opts.validate_nbf = bool_field(map, "validate_nbf", false);
    opts.validate_iat = bool_field(map, "validate_iat", false);
    opts.leeway = int_field(map, "leeway", 0).max(0) as u64;
    opts.audience = string_field(map, "audience").or_else(|| string_field(map, "aud"));
    opts.issuer = string_field(map, "issuer").or_else(|| string_field(map, "iss"));
    opts.subject = string_field(map, "subject").or_else(|| string_field(map, "sub"));
    if let Some(m) = map {
        if let Some(Value::Array(items)) = m.get("required_claims").map(|v| v.borrow().clone()) {
            opts.required_claims = items
                .into_iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s),
                    _ => None,
                })
                .collect();
        }
    }
    opts
}

fn fetch_opts_from(map: Option<&HashMap<String, ValueRef>>) -> FetchOptions {
    FetchOptions {
        timeout_ms: int_field(map, "timeout_ms", 30_000).max(1) as u64,
        user_agent: string_field(map, "user_agent"),
        max_bytes: int_field(map, "max_bytes", FetchOptions::DEFAULT_MAX_BYTES as i64).max(1) as usize,
    }
}

fn jwks_to_niao(jwks: &Jwks) -> ValueRef {
    niao_to_runtime(jwks.raw.clone())
}

// >>> njwt.sign({sub: "u1"}, "secret")
// => "eyJ..."
// >>> njwt.sign({sub: "u1"}, {secret: "secret", alg: "HS256"})
fn njwt_sign(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "njwt_sign", span)?;
    let claims = claims_arg(args, 0, "njwt_sign", span)?;
    let key = parse_key_arg(&*args[1].borrow(), span)?;
    let opts = sign_opts_from(optional_object(args, 2).as_ref());
    match sign(&claims, &key, &opts) {
        Ok(token) => Ok(Value::String(token).ref_cell()),
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.verify(token, "secret")
// => {sub: "u1", ...}
fn njwt_verify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "njwt_verify", span)?;
    let token = string_arg(args, 0, "njwt_verify", span)?;
    let key = parse_key_arg(&*args[1].borrow(), span)?;
    let opts = verify_opts_from(optional_object(args, 2).as_ref());
    match verify(&token, &key, &opts) {
        Ok(claims) => Ok(niao_to_runtime(claims)),
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.decode(token)
// => {header: {alg: "HS256"}, claims: {...}}
fn njwt_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "njwt_decode", span)?;
    let token = string_arg(args, 0, "njwt_decode", span)?;
    match decode_unverified(&token) {
        Ok((h, p)) => {
            let mut m = HashMap::new();
            m.insert("header".into(), niao_to_runtime(h));
            m.insert("claims".into(), niao_to_runtime(p));
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.header(token)
// => {alg: "HS256", typ: "JWT"}
fn njwt_header(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "njwt_header", span)?;
    let token = string_arg(args, 0, "njwt_header", span)?;
    match header(&token) {
        Ok(h) => Ok(niao_to_runtime(h)),
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.claims(token)
// => {sub: "u1", ...}
fn njwt_claims(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "njwt_claims", span)?;
    let token = string_arg(args, 0, "njwt_claims", span)?;
    match decode_unverified(&token) {
        Ok((_, p)) => Ok(niao_to_runtime(p)),
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.valid("a.b.c")
// => false
fn njwt_valid(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "njwt_valid", span)?;
    let token = string_arg(args, 0, "njwt_valid", span)?;
    Ok(Value::Bool(valid(&token)).ref_cell())
}

// >>> njwt.now() > 0
// => true
fn njwt_now(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "njwt_now", span)?;
    Ok(Value::Int(niao_jwt::now_secs() as i64).ref_cell())
}

// >>> njwt.key_from_secret("secret", "HS256")
// => {secret: "secret", alg: "HS256"}
fn njwt_key_from_secret(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "njwt_key_from_secret", span)?;
    let secret = match &*args[0].borrow() {
        Value::String(s) => s.clone(),
        Value::ByteArray(b) => String::from_utf8_lossy(b).into_owned(),
        other => {
            return Err(type_err(
                span,
                format!("njwt_key_from_secret() expects string or bytes, got {}", other.type_name()),
            ));
        }
    };
    let alg = if args.len() > 1 {
        Some(string_arg(args, 1, "njwt_key_from_secret", span)?)
    } else {
        None
    };
    match Key::from_secret(secret.as_bytes(), alg.as_deref()) {
        Ok(_) => {
            let mut m = HashMap::new();
            m.insert("secret".into(), Value::String(secret).ref_cell());
            if let Some(a) = alg {
                m.insert("alg".into(), Value::String(a).ref_cell());
            }
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.key_from_pem(pem, "RS256")
// => {pem: "-----BEGIN...", alg: "RS256"}
fn njwt_key_from_pem(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "njwt_key_from_pem", span)?;
    let pem = string_arg(args, 0, "njwt_key_from_pem", span)?;
    let alg = if args.len() > 1 {
        Some(string_arg(args, 1, "njwt_key_from_pem", span)?)
    } else {
        None
    };
    match Key::from_pem(&pem, alg.as_deref()) {
        Ok(_) => {
            let mut m = HashMap::new();
            m.insert("pem".into(), Value::String(pem).ref_cell());
            if let Some(a) = alg {
                m.insert("alg".into(), Value::String(a).ref_cell());
            }
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.jwks_parse('{"keys":[]}')
// => {keys: []}
fn njwt_jwks_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "njwt_jwks_parse", span)?;
    let json = match &*args[0].borrow() {
        Value::String(s) => s.clone(),
        Value::Object(_) => to_string(&runtime_to_niao(&*args[0].borrow(), span, "njwt_jwks_parse")?),
        other => {
            return Err(type_err(
                span,
                format!("njwt_jwks_parse() expects string or object, got {}", other.type_name()),
            ));
        }
    };
    match parse_jwks(&json) {
        Ok(jwks) => Ok(jwks_to_niao(&jwks)),
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.jwks_fetch("https://example.com/.well-known/jwks.json")
// => {keys: [...]}
fn njwt_jwks_fetch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "njwt_jwks_fetch", span)?;
    let url = string_arg(args, 0, "njwt_jwks_fetch", span)?;
    let opts = fetch_opts_from(optional_object(args, 1).as_ref());
    match fetch_jwks(&url, &opts) {
        Ok(jwks) => Ok(jwks_to_niao(&jwks)),
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.verify_jwks(token, jwks, opts)
// => {sub: "u1", ...}
fn njwt_verify_jwks(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "njwt_verify_jwks", span)?;
    let token = string_arg(args, 0, "njwt_verify_jwks", span)?;
    let jwks_json = match &*args[1].borrow() {
        Value::String(s) => s.clone(),
        Value::Object(_) => to_string(&runtime_to_niao(&*args[1].borrow(), span, "njwt_verify_jwks")?),
        other => {
            return Err(type_err(
                span,
                format!(
                    "njwt_verify_jwks() expects JWKS string or object as argument 2, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let jwks = match parse_jwks(&jwks_json) {
        Ok(j) => j,
        Err(e) => return Ok(njwt_err(span, e.message())),
    };
    let opts = verify_opts_from(optional_object(args, 2).as_ref());
    match verify_jwks(&token, &jwks, &opts) {
        Ok(claims) => Ok(niao_to_runtime(claims)),
        Err(e) => Ok(njwt_err(span, e.message())),
    }
}

// >>> njwt.verify_all([token1, token2], "secret")
// => [{sub: "a"}, {sub: "b"}]
fn njwt_verify_all(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "njwt_verify_all", span)?;
    let tokens = match &*args[0].borrow() {
        Value::Array(items) => items
            .iter()
            .map(|v| match &*v.borrow() {
                Value::String(s) => Ok(s.clone()),
                other => Err(type_err(
                    span,
                    format!("njwt_verify_all() expects string tokens, got {}", other.type_name()),
                )),
            })
            .collect::<NiaoResult<Vec<_>>>()?,
        other => {
            return Err(type_err(
                span,
                format!("njwt_verify_all() expects array as argument 1, got {}", other.type_name()),
            ));
        }
    };
    let key = parse_key_arg(&*args[1].borrow(), span)?;
    let opts = verify_opts_from(optional_object(args, 2).as_ref());
    let threads = available_threads();
    let results = verify_all(&tokens, &key, &opts, threads);
    let out: Vec<ValueRef> = results
        .into_iter()
        .map(|r| match r {
            Ok(claims) => niao_to_runtime(claims),
            Err(e) => njwt_err(span, e.message()),
        })
        .collect();
    Ok(Value::Array(out).ref_cell())
}

fn algorithms_namespace() -> Value {
    let mut map = HashMap::new();
    for name in SUPPORTED {
        map.insert((*name).to_string(), Value::String((*name).to_string()).ref_cell());
    }
    Value::Object(map)
}

macro_rules! njwt_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

njwt_fns![
    ("njwt_sign", "sign", njwt_sign),
    ("njwt_verify", "verify", njwt_verify),
    ("njwt_decode", "decode", njwt_decode),
    ("njwt_header", "header", njwt_header),
    ("njwt_claims", "claims", njwt_claims),
    ("njwt_valid", "valid", njwt_valid),
    ("njwt_now", "now", njwt_now),
    ("njwt_key_from_secret", "key_from_secret", njwt_key_from_secret),
    ("njwt_key_from_pem", "key_from_pem", njwt_key_from_pem),
    ("njwt_jwks_parse", "jwks_parse", njwt_jwks_parse),
    ("njwt_jwks_fetch", "jwks_fetch", njwt_jwks_fetch),
    ("njwt_verify_jwks", "verify_jwks", njwt_verify_jwks),
    ("njwt_verify_all", "verify_all", njwt_verify_all),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    map.insert("algorithms".into(), algorithms_namespace().ref_cell());
    Value::Object(map)
}

pub const MODULE_NAME: &str = "njwt";
pub const MODULE_PATHS: &[&str] = &["njwt", "std/njwt"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Environment;

    #[test]
    fn module_exports() {
        let ns = namespace();
        match ns {
            Value::Object(m) => {
                assert!(m.contains_key("sign"));
                assert!(m.contains_key("verify"));
                assert!(m.contains_key("algorithms"));
            }
            _ => panic!("expected object namespace"),
        }
    }
}
