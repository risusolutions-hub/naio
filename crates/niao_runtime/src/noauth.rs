//! Native noauth standard library — OAuth2 + OIDC client flows (~authlib, oauthlib subset).
//!
//! Import with `import "noauth"` (or `import "std/noauth"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_oauth::{
    auth_url, basic_auth_header, client_credentials, decode_id_token, discover,
    exchange_code, fetch_userinfo, introspect_token, is_bearer, parse_authorization_response,
    parse_callback_url, parse_discovery_json, parse_token_json, pkce_challenge, pkce_pair,
    random_nonce, random_state, refresh_token, revoke_token, token_expired, token_expires_in,
    validate_state, verify_id_token, AuthUrlOptions, ClientAuthMethod, ClientCredentialsOptions,
    ExchangeOptions, IdTokenValidation, OAuthClient, OAuthError, PkceChallengeMethod,
    RefreshOptions,
};
use niao_json_core::{Object, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4430: u32 = codes::E4440_NOAUTH_ARITY;
const E4431: u32 = codes::E4441_NOAUTH_ERROR;
const E4432: u32 = codes::E4442_NOAUTH_TYPE;
const E4433: u32 = codes::E4443_NOAUTH_INVALID_HANDLE;

thread_local! {
    static CLIENTS: RefCell<HashMap<i64, OAuthClient>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, E4432, msg.into())
}

fn noauth_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4431, "noauth_error", msg.into(), span)
}

fn map_oauth_err(span: Span, e: OAuthError) -> ValueRef {
    noauth_err(span, e.message())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4430,
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
                "{name}() expects positive handle as argument {}, got {}",
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

fn int_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        _ => None,
    }
}

fn string_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn string_list_field(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Vec<String> {
    let Some(map) = map else {
        return Vec::new();
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| {
                if let Value::String(s) = &*v.borrow() {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect(),
        Some(Value::String(s)) if !s.is_empty() => s.split_whitespace().map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

fn client_from_map(map: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<OAuthClient> {
    let map = map.ok_or_else(|| type_err(span, "client config object required"))?;
    let client_id = string_field(Some(map), "client_id")
        .ok_or_else(|| type_err(span, "client_id is required"))?;
    let token_endpoint = string_field(Some(map), "token_endpoint")
        .or_else(|| string_field(Some(map), "token_url"))
        .ok_or_else(|| type_err(span, "token_endpoint is required"))?;
    let mut b = OAuthClient::builder(client_id, token_endpoint);
    if let Some(s) = string_field(Some(map), "client_secret") {
        b = b.client_secret(s);
    }
    if let Some(u) = string_field(Some(map), "redirect_uri") {
        b = b.redirect_uri(u);
    }
    if let Some(u) = string_field(Some(map), "authorization_endpoint")
        .or_else(|| string_field(Some(map), "authorize_url"))
    {
        b = b.authorization_endpoint(u);
    }
    for key in [
        "revocation_endpoint",
        "introspection_endpoint",
        "userinfo_endpoint",
        "jwks_uri",
        "issuer",
    ] {
        if let Some(v) = string_field(Some(map), key) {
            b = match key {
                "revocation_endpoint" => b.revocation_endpoint(v),
                "introspection_endpoint" => b.introspection_endpoint(v),
                "userinfo_endpoint" => b.userinfo_endpoint(v),
                "jwks_uri" => b.jwks_uri(v),
                "issuer" => b.issuer(v),
                _ => b,
            };
        }
    }
    let scopes = string_list_field(Some(map), "scopes")
        .into_iter()
        .chain(string_list_field(Some(map), "scope"))
        .collect::<Vec<_>>();
    if !scopes.is_empty() {
        b = b.scopes(scopes);
    }
    if let Some(m) = string_field(Some(map), "client_auth_method") {
        b = b.client_auth_method(ClientAuthMethod::from_str(&m));
    }
    if let Some(ms) = int_field(Some(map), "timeout_ms") {
        if ms > 0 {
            b = b.timeout_ms(ms as u64);
        }
    }
    b.build().map_err(|e| type_err(span, e.message()))
}

fn with_client<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&OAuthClient) -> NiaoResult<ValueRef>,
{
    CLIENTS.with(|c| {
        let c = c.borrow();
        match c.get(&id) {
            Some(client) => f(client),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E4433,
        "noauth_error",
        format!("invalid or closed noauth client handle {id}"),
        span,
    )
}

fn json_object_to_niao(obj: &Object) -> HashMap<String, ValueRef> {
    let mut out = HashMap::new();
    for (k, v) in obj.iter() {
        out.insert(k.to_string(), json_to_niao(v).ref_cell());
    }
    out
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
        JsonValue::Array(items) => Value::Array(items.iter().map(json_to_niao).map(|v| v.ref_cell()).collect()),
        JsonValue::Object(o) => Value::Object(json_object_to_niao(o)),
    }
}

fn token_to_niao(t: &niao_oauth::TokenResponse) -> Value {
    let mut map = HashMap::new();
    map.insert("access_token".into(), Value::String(t.access_token.clone()).ref_cell());
    map.insert("token_type".into(), Value::String(t.token_type.clone()).ref_cell());
    if let Some(n) = t.expires_in {
        map.insert("expires_in".into(), Value::Int(n as i64).ref_cell());
    }
    if let Some(r) = &t.refresh_token {
        map.insert("refresh_token".into(), Value::String(r.clone()).ref_cell());
    }
    if let Some(s) = &t.scope {
        map.insert("scope".into(), Value::String(s.clone()).ref_cell());
    }
    if let Some(id) = &t.id_token {
        map.insert("id_token".into(), Value::String(id.clone()).ref_cell());
    }
    map.insert("obtained_at".into(), Value::Int(t.obtained_at as i64).ref_cell());
    map.insert("raw".into(), Value::Object(json_object_to_niao(&t.raw)).ref_cell());
    Value::Object(map)
}

fn pkce_to_niao(p: &niao_oauth::PkcePair) -> Value {
    let mut map = HashMap::new();
    map.insert("verifier".into(), Value::String(p.verifier.clone()).ref_cell());
    map.insert("challenge".into(), Value::String(p.challenge.clone()).ref_cell());
    map.insert(
        "method".into(),
        Value::String(p.method.as_str().to_string()).ref_cell(),
    );
    Value::Object(map)
}

fn auth_response_to_niao(r: &niao_oauth::AuthorizationResponse) -> Value {
    let mut map = HashMap::new();
    if let Some(c) = &r.code {
        map.insert("code".into(), Value::String(c.clone()).ref_cell());
    }
    if let Some(s) = &r.state {
        map.insert("state".into(), Value::String(s.clone()).ref_cell());
    }
    if let Some(e) = &r.error {
        map.insert("error".into(), Value::String(e.clone()).ref_cell());
    }
    if let Some(d) = &r.error_description {
        map.insert("error_description".into(), Value::String(d.clone()).ref_cell());
    }
    if let Some(u) = &r.error_uri {
        map.insert("error_uri".into(), Value::String(u.clone()).ref_cell());
    }
    Value::Object(map)
}

fn auth_url_opts_from_map(map: Option<&HashMap<String, ValueRef>>) -> AuthUrlOptions {
    let mut opts = AuthUrlOptions::default();
    opts.state = string_field(map, "state");
    opts.nonce = string_field(map, "nonce");
    opts.code_challenge = string_field(map, "code_challenge");
    if let Some(m) = string_field(map, "code_challenge_method") {
        opts.code_challenge_method = PkceChallengeMethod::from_str(&m);
    }
    let scopes = string_list_field(map, "scopes");
    if !scopes.is_empty() {
        opts.scopes = Some(scopes);
    }
    opts.response_mode = string_field(map, "response_mode");
    opts.prompt = string_field(map, "prompt");
    opts.login_hint = string_field(map, "login_hint");
    opts.audience = string_field(map, "audience");
    opts
}

fn id_validation_from_map(map: Option<&HashMap<String, ValueRef>>) -> IdTokenValidation {
    let mut v = IdTokenValidation::default();
    v.issuer = string_field(map, "issuer");
    v.audience = string_field(map, "audience");
    v.nonce = string_field(map, "nonce");
    if let Some(n) = int_field(map, "leeway") {
        v.leeway = n.max(0) as u64;
    }
    v.validate_exp = bool_field(map, "validate_exp", true);
    v.validate_nbf = bool_field(map, "validate_nbf", false);
    if let Some(n) = int_field(map, "max_age") {
        v.max_age = Some(n.max(0) as u64);
    }
    v
}

// >>> noauth.random_state()
fn noauth_random_state(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::String(random_state()).ref_cell())
}

// >>> len(noauth.random_nonce()) >= 32
fn noauth_random_nonce(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::String(random_nonce()).ref_cell())
}

// >>> noauth.pkce().method == "S256"
fn noauth_pkce(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "noauth_pkce", span)?;
    let use_s256 = if args.is_empty() {
        true
    } else {
        bool_field(optional_object(args, 0), "s256", true)
    };
    Ok(pkce_to_niao(&pkce_pair(use_s256)).ref_cell())
}

// >>> noauth.pkce_challenge("abc", "plain") == "abc"
fn noauth_pkce_challenge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "noauth_pkce_challenge", span)?;
    let verifier = string_arg(args, 0, "noauth_pkce_challenge", span)?;
    let method = if args.len() == 2 {
        match &*args[1].borrow() {
            Value::String(s) => PkceChallengeMethod::from_str(s).unwrap_or(PkceChallengeMethod::S256),
            other => {
                return Err(type_err(
                    span,
                    format!("method must be string, got {}", other.type_name()),
                ));
            }
        }
    } else {
        PkceChallengeMethod::S256
    };
    Ok(Value::String(pkce_challenge(&verifier, method)).ref_cell())
}

// >>> type(noauth.client({client_id: "x", token_endpoint: "https://t"})) == "int"
fn noauth_client(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_client", span)?;
    match client_from_map(optional_object(args, 0), span) {
        Ok(client) => {
            let id = new_id();
            CLIENTS.with(|c| c.borrow_mut().insert(id, client));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(noauth_err(span, e.message())),
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4430,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn noauth_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_close", span)?;
    let id = handle_arg(args, 0, "noauth_close", span)?;
    CLIENTS.with(|c| {
        c.borrow_mut().remove(&id);
    });
    Ok(Value::Nil.ref_cell())
}

fn noauth_client_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_client_info", span)?;
    let id = handle_arg(args, 0, "noauth_client_info", span)?;
    with_client(id, span, |c| {
        let mut map = HashMap::new();
        map.insert("client_id".into(), Value::String(c.client_id.clone()).ref_cell());
        if let Some(s) = &c.client_secret {
            map.insert("client_secret".into(), Value::String(s.clone()).ref_cell());
        }
        if let Some(u) = &c.redirect_uri {
            map.insert("redirect_uri".into(), Value::String(u.clone()).ref_cell());
        }
        map.insert(
            "authorization_endpoint".into(),
            Value::String(c.authorization_endpoint.clone()).ref_cell(),
        );
        map.insert(
            "token_endpoint".into(),
            Value::String(c.token_endpoint.clone()).ref_cell(),
        );
        if let Some(u) = &c.issuer {
            map.insert("issuer".into(), Value::String(u.clone()).ref_cell());
        }
        Ok(Value::Object(map).ref_cell())
    })
}

fn noauth_discover(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_discover", span)?;
    let issuer = string_arg(args, 0, "noauth_discover", span)?;
    match discover(&issuer) {
        Ok(cfg) => {
            let mut map = HashMap::new();
            map.insert("issuer".into(), Value::String(cfg.issuer).ref_cell());
            map.insert(
                "authorization_endpoint".into(),
                Value::String(cfg.authorization_endpoint).ref_cell(),
            );
            map.insert("token_endpoint".into(), Value::String(cfg.token_endpoint).ref_cell());
            if let Some(u) = cfg.userinfo_endpoint {
                map.insert("userinfo_endpoint".into(), Value::String(u).ref_cell());
            }
            if let Some(u) = cfg.jwks_uri {
                map.insert("jwks_uri".into(), Value::String(u).ref_cell());
            }
            map.insert("raw".into(), Value::Object(json_object_to_niao(&cfg.raw)).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_oauth_err(span, e)),
    }
}

fn noauth_client_from_discovery(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "noauth_client_from_discovery", span)?;
    let discovery = match &*args[0].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!("discovery must be object, got {}", other.type_name()),
            ));
        }
    };
    let client_id = string_arg(args, 1, "noauth_client_from_discovery", span)?;
    let raw_json = if let Some(v) = discovery.get("raw") {
        match &*v.borrow() {
            Value::Object(_) => serde_json_from_niao_object(&*v.borrow(), span)?,
            _ => return Err(type_err(span, "discovery.raw must be object")),
        }
    } else {
        return Err(type_err(span, "discovery object missing raw metadata"));
    };
    let cfg = parse_discovery_json(&raw_json).map_err(|e| type_err(span, e.message()))?;
    let mut b = cfg.client_builder(client_id).map_err(|e| type_err(span, e.message()))?;
    if let Some(extra) = optional_object(args, 2) {
        if let Some(s) = string_field(Some(&extra), "client_secret") {
            b = b.client_secret(s);
        }
        if let Some(u) = string_field(Some(&extra), "redirect_uri") {
            b = b.redirect_uri(u);
        }
        let scopes = string_list_field(Some(&extra), "scopes");
        if !scopes.is_empty() {
            b = b.scopes(scopes);
        }
    }
    let client = b.build().map_err(|e| type_err(span, e.message()))?;
    let id = new_id();
    CLIENTS.with(|c| c.borrow_mut().insert(id, client));
    Ok(Value::Int(id).ref_cell())
}

fn serde_json_from_niao_object(v: &Value, span: Span) -> NiaoResult<String> {
    match v {
        Value::Object(map) => {
            let mut pairs = Vec::new();
            for (k, vr) in map {
                pairs.push(format!(
                    "\"{}\":{}",
                    k,
                    niao_value_to_json_fragment(&*vr.borrow())
                ));
            }
            Ok(format!("{{{}}}", pairs.join(",")))
        }
        _ => Err(type_err(span, "expected object")),
    }
}

fn niao_value_to_json_fragment(v: &Value) -> String {
    match v {
        Value::Nil => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(items) => {
            let inner: Vec<_> = items.iter().map(|i| niao_value_to_json_fragment(&*i.borrow())).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Object(map) => {
            let pairs: Vec<_> = map
                .iter()
                .map(|(k, vr)| format!("\"{k}\":{}", niao_value_to_json_fragment(&*vr.borrow())))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        other => format!("\"{}\"", other.type_name()),
    }
}

fn noauth_auth_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "noauth_auth_url", span)?;
    let id = handle_arg(args, 0, "noauth_auth_url", span)?;
    let opts = auth_url_opts_from_map(optional_object(args, 1));
    with_client(id, span, |c| match auth_url(c, &opts) {
        Ok(url) => Ok(Value::String(url).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn noauth_exchange_code(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "noauth_exchange_code", span)?;
    let id = handle_arg(args, 0, "noauth_exchange_code", span)?;
    let code = string_arg(args, 1, "noauth_exchange_code", span)?;
    let map = optional_object(args, 2);
    let mut opts = ExchangeOptions::default();
    opts.code_verifier = string_field(map.as_ref(), "code_verifier");
    opts.redirect_uri = string_field(map.as_ref(), "redirect_uri");
    with_client(id, span, |c| match exchange_code(c, &code, &opts) {
        Ok(t) => Ok(token_to_niao(&t).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn noauth_client_credentials(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "noauth_client_credentials", span)?;
    let id = handle_arg(args, 0, "noauth_client_credentials", span)?;
    let map = optional_object(args, 1);
    let mut opts = ClientCredentialsOptions::default();
    opts.scope = string_field(map.as_ref(), "scope");
    opts.audience = string_field(map.as_ref(), "audience");
    with_client(id, span, |c| match client_credentials(c, &opts) {
        Ok(t) => Ok(token_to_niao(&t).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn noauth_refresh(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "noauth_refresh", span)?;
    let id = handle_arg(args, 0, "noauth_refresh", span)?;
    let refresh = string_arg(args, 1, "noauth_refresh", span)?;
    let map = optional_object(args, 2);
    let mut opts = RefreshOptions::default();
    opts.scope = string_field(map.as_ref(), "scope");
    with_client(id, span, |c| match refresh_token(c, &refresh, &opts) {
        Ok(t) => Ok(token_to_niao(&t).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn noauth_revoke(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "noauth_revoke", span)?;
    let id = handle_arg(args, 0, "noauth_revoke", span)?;
    let token = string_arg(args, 1, "noauth_revoke", span)?;
    let hint = optional_object(args, 2).and_then(|m| string_field(Some(&m), "token_type_hint"));
    with_client(id, span, |c| match revoke_token(c, &token, hint.as_deref()) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn noauth_introspect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "noauth_introspect", span)?;
    let id = handle_arg(args, 0, "noauth_introspect", span)?;
    let token = string_arg(args, 1, "noauth_introspect", span)?;
    with_client(id, span, |c| match introspect_token(c, &token) {
        Ok(obj) => Ok(Value::Object(json_object_to_niao(&obj)).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn noauth_userinfo(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "noauth_userinfo", span)?;
    let id = handle_arg(args, 0, "noauth_userinfo", span)?;
    let access = string_arg(args, 1, "noauth_userinfo", span)?;
    with_client(id, span, |c| match fetch_userinfo(c, &access) {
        Ok(obj) => Ok(Value::Object(json_object_to_niao(&obj)).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn noauth_parse_callback(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_parse_callback", span)?;
    let url = string_arg(args, 0, "noauth_parse_callback", span)?;
    match parse_callback_url(&url) {
        Ok(r) => Ok(auth_response_to_niao(&r).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    }
}

fn noauth_parse_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_parse_query", span)?;
    let q = string_arg(args, 0, "noauth_parse_query", span)?;
    match parse_authorization_response(&q) {
        Ok(r) => Ok(auth_response_to_niao(&r).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    }
}

fn noauth_validate_state(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "noauth_validate_state", span)?;
    let expected = string_arg(args, 0, "noauth_validate_state", span)?;
    let received = string_arg(args, 1, "noauth_validate_state", span)?;
    match validate_state(&expected, &received) {
        Ok(()) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    }
}

fn noauth_parse_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_parse_token", span)?;
    let json = string_arg(args, 0, "noauth_parse_token", span)?;
    match parse_token_json(&json) {
        Ok(t) => Ok(token_to_niao(&t).ref_cell()),
        Err(e) => Ok(map_oauth_err(span, e)),
    }
}

fn noauth_token_expired(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "noauth_token_expired", span)?;
    let token = token_from_arg(args, 0, span)?;
    let leeway = int_field(optional_object(args, 1), "leeway").unwrap_or(0).max(0) as u64;
    Ok(Value::Bool(token_expired(&token, leeway)).ref_cell())
}

fn noauth_token_expires_in(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_token_expires_in", span)?;
    let token = token_from_arg(args, 0, span)?;
    Ok(match token_expires_in(&token) {
        Some(n) => Value::Int(n as i64).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
}

fn noauth_is_bearer(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_is_bearer", span)?;
    let token = token_from_arg(args, 0, span)?;
    Ok(Value::Bool(is_bearer(&token)).ref_cell())
}

fn noauth_basic_auth(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "noauth_basic_auth", span)?;
    let id = string_arg(args, 0, "noauth_basic_auth", span)?;
    let secret = string_arg(args, 1, "noauth_basic_auth", span)?;
    Ok(Value::String(basic_auth_header(&id, &secret)).ref_cell())
}

fn noauth_decode_id_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "noauth_decode_id_token", span)?;
    let token = string_arg(args, 0, "noauth_decode_id_token", span)?;
    match decode_id_token(&token) {
        Ok((header, claims)) => {
            let mut map = HashMap::new();
            map.insert("header".into(), Value::Object(json_object_to_niao(&header)).ref_cell());
            map.insert("claims".into(), Value::Object(json_object_to_niao(&claims)).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_oauth_err(span, e)),
    }
}

fn noauth_verify_id_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "noauth_verify_id_token", span)?;
    let id = handle_arg(args, 0, "noauth_verify_id_token", span)?;
    let token = string_arg(args, 1, "noauth_verify_id_token", span)?;
    let validation = id_validation_from_map(optional_object(args, 2));
    with_client(id, span, |c| match verify_id_token(c, &token, &validation) {
        Ok(v) => {
            let mut map = HashMap::new();
            map.insert("header".into(), Value::Object(json_object_to_niao(&v.header)).ref_cell());
            map.insert("claims".into(), Value::Object(json_object_to_niao(&v.claims)).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(map_oauth_err(span, e)),
    })
}

fn token_from_arg(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<niao_oauth::TokenResponse> {
    match &*args[idx].borrow() {
        Value::Object(map) => {
            let access = map
                .get("access_token")
                .and_then(|v| {
                    if let Value::String(s) = &*v.borrow() {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| type_err(span, "token object missing access_token"))?;
            let token_type = map
                .get("token_type")
                .and_then(|v| {
                    if let Value::String(s) = &*v.borrow() {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "Bearer".into());
            let expires_in = map.get("expires_in").and_then(|v| {
                if let Value::Int(n) = &*v.borrow() {
                    Some(*n as u64)
                } else {
                    None
                }
            });
            let obtained_at = map
                .get("obtained_at")
                .and_then(|v| {
                    if let Value::Int(n) = &*v.borrow() {
                        Some(*n as u64)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            Ok(niao_oauth::TokenResponse {
                access_token: access,
                token_type,
                expires_in,
                refresh_token: None,
                scope: None,
                id_token: None,
                obtained_at,
                raw: Object::new(),
            })
        }
        other => Err(type_err(
            span,
            format!("expected token object, got {}", other.type_name()),
        )),
    }
}

fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
    vec![
        ("noauth_random_state", "random_state", Rc::new(noauth_random_state)),
        ("noauth_random_nonce", "random_nonce", Rc::new(noauth_random_nonce)),
        ("noauth_pkce", "pkce", Rc::new(noauth_pkce)),
        ("noauth_pkce_challenge", "pkce_challenge", Rc::new(noauth_pkce_challenge)),
        ("noauth_client", "client", Rc::new(noauth_client)),
        ("noauth_close", "close", Rc::new(noauth_close)),
        ("noauth_client_info", "client_info", Rc::new(noauth_client_info)),
        ("noauth_discover", "discover", Rc::new(noauth_discover)),
        ("noauth_client_from_discovery", "client_from_discovery", Rc::new(noauth_client_from_discovery)),
        ("noauth_auth_url", "auth_url", Rc::new(noauth_auth_url)),
        ("noauth_exchange_code", "exchange_code", Rc::new(noauth_exchange_code)),
        ("noauth_client_credentials", "client_credentials", Rc::new(noauth_client_credentials)),
        ("noauth_refresh", "refresh", Rc::new(noauth_refresh)),
        ("noauth_revoke", "revoke", Rc::new(noauth_revoke)),
        ("noauth_introspect", "introspect", Rc::new(noauth_introspect)),
        ("noauth_userinfo", "userinfo", Rc::new(noauth_userinfo)),
        ("noauth_parse_callback", "parse_callback", Rc::new(noauth_parse_callback)),
        ("noauth_parse_query", "parse_query", Rc::new(noauth_parse_query)),
        ("noauth_validate_state", "validate_state", Rc::new(noauth_validate_state)),
        ("noauth_parse_token", "parse_token", Rc::new(noauth_parse_token)),
        ("noauth_token_expired", "token_expired", Rc::new(noauth_token_expired)),
        ("noauth_token_expires_in", "token_expires_in", Rc::new(noauth_token_expires_in)),
        ("noauth_is_bearer", "is_bearer", Rc::new(noauth_is_bearer)),
        ("noauth_basic_auth", "basic_auth", Rc::new(noauth_basic_auth)),
        ("noauth_decode_id_token", "decode_id_token", Rc::new(noauth_decode_id_token)),
        ("noauth_verify_id_token", "verify_id_token", Rc::new(noauth_verify_id_token)),
    ]
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "noauth";
pub const MODULE_PATHS: &[&str] = &["noauth", "std/noauth"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_doctest() {
        let v = noauth_pkce(&[], Span::dummy()).unwrap();
        match &*v.borrow() {
            Value::Object(m) => {
                assert!(m.contains_key("verifier"));
                assert!(m.contains_key("challenge"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
