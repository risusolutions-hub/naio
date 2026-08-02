//! Native `nauth` standard library — web auth kit: sessions, login/logout,
//! password reset, RBAC, CSRF (~flask-login, django auth subset; npass + nsign).
//!
//! Import with `import "nauth"` (or `import "std/nauth"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_auth::{
    allows, anonymous_user, compare, context_from_opts, expand_roles, extract_cookie, generate_token,
    has_permission, has_role, hash_password,
    user as make_user, verify_and_update, verify_password, Auth, AuthConfig, AuthError,
    RoleHierarchy, SessionData, DEFAULT_COOKIE_NAME, DEFAULT_RESET_MAX_AGE,
    DEFAULT_SESSION_LIFETIME, DEFAULT_TOKEN_BYTES,
};
use niao_errors::codes;
use niao_json_core::{Object as JsonObject, Value as JsonValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4494_NAUTH_ARITY: u32 = codes::E4494_NAUTH_ARITY;
const E4495_NAUTH_ERROR: u32 = codes::E4495_NAUTH_ERROR;
const E4496_NAUTH_TYPE: u32 = codes::E4496_NAUTH_TYPE;
const E4497_NAUTH_INVALID_HANDLE: u32 = codes::E4497_NAUTH_INVALID_HANDLE;
const E4498_NAUTH_AUTH: u32 = codes::E4498_NAUTH_AUTH;
const E4499_NAUTH_FORBIDDEN: u32 = codes::E4499_NAUTH_FORBIDDEN;
const E4500_NAUTH_CSRF: u32 = codes::E4500_NAUTH_CSRF;
const E4501_NAUTH_EXPIRED: u32 = codes::E4501_NAUTH_EXPIRED;

enum NauthHandle {
    Auth(Auth),
    Session { auth_id: i64, data: SessionData },
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, NauthHandle>> = RefCell::new(HashMap::new());
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

fn register(handle: NauthHandle) -> i64 {
    let id = new_handle();
    HANDLES.with(|m| m.borrow_mut().insert(id, handle));
    id
}

fn with_handle<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut NauthHandle) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    HANDLES.with(|m| {
        let mut m = m.borrow_mut();
        match m.get_mut(&id) {
            Some(h) => Ok(Ok(f(h))),
            None => Ok(Err(error_value(
                E4497_NAUTH_INVALID_HANDLE,
                "nauth_error",
                format!("invalid or closed nauth handle {id}"),
                span,
            ))),
        }
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4496_NAUTH_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4494_NAUTH_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4494_NAUTH_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn map_err(span: Span, e: AuthError) -> ValueRef {
    let (code, kind) = match &e {
        AuthError::BadCredentials => (E4498_NAUTH_AUTH, "nauth_error"),
        AuthError::Forbidden(_) => (E4499_NAUTH_FORBIDDEN, "nauth_error"),
        AuthError::CsrfMismatch => (E4500_NAUTH_CSRF, "nauth_error"),
        AuthError::Expired(_) => (E4501_NAUTH_EXPIRED, "nauth_error"),
        _ => (E4495_NAUTH_ERROR, "nauth_error"),
    };
    error_value(code, kind, e.to_string(), span)
}

fn nauth_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E4495_NAUTH_ERROR, "nauth_error", msg.into(), span)
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

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        Value::Nil => default,
        _ => default,
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
            format!("opts must be an object, got {}", other.type_name()),
        )),
    }
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            Value::Int(n) => Some(*n != 0),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_string(map: &HashMap<String, ValueRef>, key: &str, default: &str) -> String {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| default.to_string())
}

fn obj_string_opt(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn string_list_from_val(v: &ValueRef) -> Vec<String> {
    match &*v.borrow() {
        Value::Array(items) => items
            .iter()
            .filter_map(|i| match &*i.borrow() {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn obj_string_list(map: &HashMap<String, ValueRef>, key: &str) -> Vec<String> {
    map.get(key)
        .map(string_list_from_val)
        .unwrap_or_default()
}

fn string_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects string array elements, got {}",
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
                "{name}() expects a string array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn parse_hierarchy(map: &HashMap<String, ValueRef>) -> RoleHierarchy {
    let Some(v) = map.get("roles") else {
        return RoleHierarchy::new();
    };
    let Value::Object(obj) = &*v.borrow() else {
        return RoleHierarchy::new();
    };
    let mut h = RoleHierarchy::new();
    for (k, val) in obj {
        h.insert(k.clone(), string_list_from_val(val));
    }
    h
}

fn hierarchy_from_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<RoleHierarchy> {
    match &*args[idx].borrow() {
        Value::Object(obj) => {
            let mut h = RoleHierarchy::new();
            for (k, val) in obj {
                h.insert(k.clone(), string_list_from_val(val));
            }
            Ok(h)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects hierarchy object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_id_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Object(map) => match map.get("id") {
            Some(v) => match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(type_err(
                    span,
                    format!("{name}() handle.id must be int, got {}", other.type_name()),
                )),
            },
            None => Err(type_err(span, format!("{name}() missing handle id"))),
        },
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a handle object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn json_to_niao(v: &JsonValue) -> ValueRef {
    match v {
        JsonValue::Null => Value::Nil.ref_cell(),
        JsonValue::Bool(b) => bool_val(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                int_val(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f).ref_cell()
            } else {
                Value::Nil.ref_cell()
            }
        }
        JsonValue::String(s) => str_val(s.clone()),
        JsonValue::Array(items) => {
            Value::Array(items.iter().map(json_to_niao).collect()).ref_cell()
        }
        JsonValue::Object(obj) => {
            let mut map = HashMap::new();
            for (k, val) in obj.iter() {
                map.insert(k.to_string(), json_to_niao(val));
            }
            Value::Object(map).ref_cell()
        }
    }
}

fn niao_to_json(v: &Value) -> JsonValue {
    match v {
        Value::Nil => JsonValue::Null,
        Value::Bool(b) => JsonValue::bool(*b),
        Value::Int(n) => JsonValue::int(*n),
        Value::Float(f) => JsonValue::float(*f),
        Value::String(s) => JsonValue::string(s.clone()),
        Value::Array(items) => JsonValue::array(
            items
                .iter()
                .map(|i| niao_to_json(&i.borrow()))
                .collect(),
        ),
        Value::Object(map) => {
            let mut obj = JsonObject::new();
            for (k, val) in map {
                obj.insert(k.clone(), niao_to_json(&val.borrow()));
            }
            JsonValue::object(obj)
        }
        _ => JsonValue::Null,
    }
}

fn string_array_val(items: &[String]) -> ValueRef {
    Value::Array(items.iter().map(|s| str_val(s.clone())).collect()).ref_cell()
}

fn session_object(id: i64, data: &SessionData) -> ValueRef {
    let mut fields = HashMap::new();
    fields.insert(
        "token".to_string(),
        Value::NativeFunction(Rc::new(nauth_session_token)).ref_cell(),
    );
    fields.insert(
        "cookie".to_string(),
        Value::NativeFunction(Rc::new(nauth_session_cookie_method)).ref_cell(),
    );
    fields.insert(
        "get".to_string(),
        Value::NativeFunction(Rc::new(nauth_session_get)).ref_cell(),
    );
    fields.insert(
        "set".to_string(),
        Value::NativeFunction(Rc::new(nauth_session_set)).ref_cell(),
    );
    fields.insert(
        "refresh".to_string(),
        Value::NativeFunction(Rc::new(nauth_session_refresh)).ref_cell(),
    );
    fields.insert(
        "to_object".to_string(),
        Value::NativeFunction(Rc::new(nauth_session_to_object)).ref_cell(),
    );
    fields.insert("id".to_string(), int_val(id));
    fields.insert("kind".to_string(), str_val("session"));
    fields.insert("user_id".to_string(), str_val(data.user_id.clone()));
    fields.insert("session_id".to_string(), str_val(data.session_id.clone()));
    fields.insert("roles".to_string(), string_array_val(&data.roles));
    fields.insert("permissions".to_string(), string_array_val(&data.permissions));
    fields.insert("is_authenticated".to_string(), bool_val(true));
    Value::Object(fields).ref_cell()
}

fn auth_object(id: i64) -> ValueRef {
    let mut fields = HashMap::new();
    let methods: &[(&str, NativeFn)] = &[
        ("hash_password", Rc::new(nauth_auth_hash_password)),
        ("verify_password", Rc::new(nauth_auth_verify_password)),
        ("verify_and_update", Rc::new(nauth_auth_verify_and_update)),
        ("login", Rc::new(nauth_auth_login)),
        ("login_user", Rc::new(nauth_auth_login_user)),
        ("create_session", Rc::new(nauth_auth_login_user)),
        ("logout", Rc::new(nauth_auth_logout)),
        ("load_session", Rc::new(nauth_auth_load_session)),
        ("session_from_cookie", Rc::new(nauth_auth_session_from_cookie)),
        ("cookie", Rc::new(nauth_auth_cookie)),
        ("reset_token", Rc::new(nauth_auth_reset_token)),
        ("verify_reset", Rc::new(nauth_auth_verify_reset)),
        ("complete_reset", Rc::new(nauth_auth_complete_reset)),
        ("csrf_token", Rc::new(nauth_auth_csrf_token)),
        ("validate_csrf", Rc::new(nauth_auth_validate_csrf)),
        ("allows", Rc::new(nauth_auth_allows)),
        ("expand_roles", Rc::new(nauth_auth_expand_roles)),
        ("has_permission", Rc::new(nauth_auth_has_permission)),
    ];
    for (name, f) in methods {
        fields.insert(name.to_string(), Value::NativeFunction(f.clone()).ref_cell());
    }
    fields.insert("id".to_string(), int_val(id));
    fields.insert("kind".to_string(), str_val("auth"));
    Value::Object(fields).ref_cell()
}

fn build_auth_from_opts(secret: &str, opts: &HashMap<String, ValueRef>, span: Span) -> Result<Auth, ValueRef> {
    let mut cfg = AuthConfig::new(secret.as_bytes()).map_err(|e| map_err(span, e))?;
    cfg.cookie_name = obj_string(opts, "cookie_name", DEFAULT_COOKIE_NAME);
    cfg.session_lifetime = obj_int(opts, "session_lifetime", DEFAULT_SESSION_LIFETIME as i64).max(0) as u64;
    cfg.reset_max_age = obj_int(opts, "reset_max_age", DEFAULT_RESET_MAX_AGE as i64).max(0) as u64;
    cfg.cookie_path = obj_string(opts, "cookie_path", "/");
    cfg.cookie_http_only = obj_bool(opts, "cookie_http_only", true);
    cfg.cookie_secure = obj_bool(opts, "cookie_secure", false);
    cfg.cookie_same_site = obj_string(opts, "cookie_same_site", "Lax");
    cfg.hierarchy = parse_hierarchy(opts);
    let scheme = obj_string_opt(opts, "scheme");
    let bcrypt_cost = {
        let c = obj_int(opts, "bcrypt_cost", -1);
        if c >= 0 {
            Some(c as u32)
        } else {
            None
        }
    };
    let memory_kib = {
        let m = obj_int(opts, "memory_kib", -1);
        if m >= 0 {
            Some(m as u32)
        } else {
            None
        }
    };
    let time_cost = {
        let t = obj_int(opts, "time_cost", -1);
        if t >= 0 {
            Some(t as u32)
        } else {
            None
        }
    };
    if scheme.is_some() || bcrypt_cost.is_some() || memory_kib.is_some() || time_cost.is_some() {
        cfg.pass_ctx = context_from_opts(scheme.as_deref(), bcrypt_cost, memory_kib, time_cost)
            .map_err(|e| map_err(span, e))?;
    }
    Ok(Auth::with_config(cfg))
}

fn register_session(auth_id: i64, data: SessionData) -> ValueRef {
    let id = register(NauthHandle::Session { auth_id, data: data.clone() });
    session_object(id, &data)
}

fn sid_from_arg(args: &[ValueRef], idx: usize, span: Span, name: &str) -> NiaoResult<Result<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(Err(nauth_err(span, format!("{name}() missing session id"))));
    }
    match &*args[idx].borrow() {
        Value::String(s) => Ok(Ok(s.clone())),
        Value::Object(map) => {
            if let Some(v) = map.get("session_id") {
                if let Value::String(s) = &*v.borrow() {
                    return Ok(Ok(s.clone()));
                }
            }
            // Fall back: look up handle
            if let Some(v) = map.get("id") {
                if let Value::Int(id) = &*v.borrow() {
                    return match with_handle(*id, span, |h| match h {
                        NauthHandle::Session { data, .. } => Ok(data.session_id.clone()),
                        _ => Err(nauth_err(span, "expected session handle")),
                    })? {
                        Ok(Ok(s)) => Ok(Ok(s)),
                        Ok(Err(e)) => Ok(Err(e)),
                        Err(e) => Ok(Err(e)),
                    };
                }
            }
            Ok(Err(nauth_err(span, format!("{name}() expected session or session_id"))))
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or session, got {}",
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Module functions
// ---------------------------------------------------------------------------

// >>> type(nauth.auth("dev-secret-change-me-now"))
// "object"
fn nauth_auth(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nauth.auth", span)?;
    let secret = string_arg(args, 0, "nauth.auth", span)?;
    let opts = parse_opts(args, 1, span)?;
    match build_auth_from_opts(&secret, &opts, span) {
        Ok(auth) => {
            let id = register(NauthHandle::Auth(auth));
            Ok(auth_object(id))
        }
        Err(e) => Ok(e),
    }
}

// >>> type(nauth.hash_password("x", {scheme: "bcrypt", bcrypt_cost: 4}))
// "string"
fn nauth_hash_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nauth.hash_password", span)?;
    let password = string_arg(args, 0, "nauth.hash_password", span)?;
    let opts = parse_opts(args, 1, span)?;
    let scheme = obj_string_opt(&opts, "scheme");
    let bcrypt_cost = {
        let c = obj_int(&opts, "bcrypt_cost", -1);
        if c >= 0 { Some(c as u32) } else { None }
    };
    let memory_kib = {
        let m = obj_int(&opts, "memory_kib", -1);
        if m >= 0 { Some(m as u32) } else { None }
    };
    let time_cost = {
        let t = obj_int(&opts, "time_cost", -1);
        if t >= 0 { Some(t as u32) } else { None }
    };
    if scheme.is_some() || bcrypt_cost.is_some() || memory_kib.is_some() || time_cost.is_some() {
        match context_from_opts(scheme.as_deref(), bcrypt_cost, memory_kib, time_cost) {
            Ok(ctx) => match niao_auth::hash_with(&ctx, &password) {
                Ok(h) => Ok(str_val(h)),
                Err(e) => Ok(map_err(span, e)),
            },
            Err(e) => Ok(map_err(span, e)),
        }
    } else {
        match hash_password(&password) {
            Ok(h) => Ok(str_val(h)),
            Err(e) => Ok(map_err(span, e)),
        }
    }
}

// >>> nauth.verify_password("x", nauth.hash_password("x", {scheme: "bcrypt", bcrypt_cost: 4}))
// true
fn nauth_verify_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nauth.verify_password", span)?;
    let password = string_arg(args, 0, "nauth.verify_password", span)?;
    let hash = string_arg(args, 1, "nauth.verify_password", span)?;
    match verify_password(&password, &hash) {
        Ok(b) => Ok(bool_val(b)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> let r = nauth.verify_and_update("x", nauth.hash_password("x", {scheme: "bcrypt", bcrypt_cost: 4}), {scheme: "bcrypt", bcrypt_cost: 4}); r.ok
// true
fn nauth_verify_and_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nauth.verify_and_update", span)?;
    let password = string_arg(args, 0, "nauth.verify_and_update", span)?;
    let hash = string_arg(args, 1, "nauth.verify_and_update", span)?;
    let opts = parse_opts(args, 2, span)?;
    let scheme = obj_string_opt(&opts, "scheme");
    let bcrypt_cost = {
        let c = obj_int(&opts, "bcrypt_cost", -1);
        if c >= 0 { Some(c as u32) } else { None }
    };
    let ctx = match context_from_opts(scheme.as_deref(), bcrypt_cost, None, None) {
        Ok(c) => c,
        Err(e) => return Ok(map_err(span, e)),
    };
    match verify_and_update(&ctx, &password, &hash) {
        Ok(vu) => {
            let mut m = HashMap::new();
            m.insert("ok".into(), bool_val(vu.ok));
            m.insert("updated".into(), bool_val(vu.updated));
            m.insert(
                "hash".into(),
                match vu.hash {
                    Some(h) => str_val(h),
                    None => Value::Nil.ref_cell(),
                },
            );
            Ok(Value::Object(m).ref_cell())
        }
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nauth.user("u1", {roles: ["admin"]}).is_authenticated
// true
fn nauth_user(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nauth.user", span)?;
    let id = string_arg(args, 0, "nauth.user", span)?;
    let opts = parse_opts(args, 1, span)?;
    let roles = obj_string_list(&opts, "roles");
    let perms = obj_string_list(&opts, "permissions");
    let active = obj_bool(&opts, "active", true);
    Ok(json_to_niao(&make_user(&id, &roles, &perms, active)))
}

// >>> nauth.anonymous().is_anonymous
// true
fn nauth_anonymous(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nauth.anonymous", span)?;
    Ok(json_to_niao(&anonymous_user()))
}

// >>> nauth.compare("ab", "ab")
// true
fn nauth_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nauth.compare", span)?;
    let a = string_arg(args, 0, "nauth.compare", span)?;
    let b = string_arg(args, 1, "nauth.compare", span)?;
    Ok(bool_val(compare(&a, &b)))
}

// >>> len(nauth.token(16)) > 0
// true
fn nauth_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nauth.token", span)?;
    let n = optional_int(args, 0, DEFAULT_TOKEN_BYTES as i64);
    if n <= 0 {
        return Ok(nauth_err(span, "token nbytes must be > 0"));
    }
    match generate_token(n as usize) {
        Ok(t) => Ok(str_val(t)),
        Err(e) => Ok(map_err(span, e)),
    }
}

// >>> nauth.has_role(["admin", "user"], "admin")
// true
fn nauth_has_role(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nauth.has_role", span)?;
    let roles = string_array_arg(args, 0, "nauth.has_role", span)?;
    let role = string_arg(args, 1, "nauth.has_role", span)?;
    Ok(bool_val(has_role(&roles, &role)))
}

// >>> nauth.has_permission(["read", "*"], "write")
// true
fn nauth_has_permission(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nauth.has_permission", span)?;
    let perms = string_array_arg(args, 0, "nauth.has_permission", span)?;
    let perm = string_arg(args, 1, "nauth.has_permission", span)?;
    Ok(bool_val(has_permission(&perms, &perm)))
}

// >>> let e = nauth.roles_expand({admin: ["editor"]}, ["admin"]); nauth.has_role(e, "editor")
// true
fn nauth_roles_expand(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nauth.roles_expand", span)?;
    let h = hierarchy_from_arg(args, 0, "nauth.roles_expand", span)?;
    let roles = string_array_arg(args, 1, "nauth.roles_expand", span)?;
    Ok(string_array_val(&expand_roles(&h, &roles)))
}

// >>> nauth.roles_allows({admin: ["viewer"]}, ["admin"], "viewer")
// true
fn nauth_roles_allows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nauth.roles_allows", span)?;
    let h = hierarchy_from_arg(args, 0, "nauth.roles_allows", span)?;
    let roles = string_array_arg(args, 1, "nauth.roles_allows", span)?;
    let required = string_arg(args, 2, "nauth.roles_allows", span)?;
    Ok(bool_val(allows(&h, &roles, &required)))
}

// >>> nauth.extract_cookie("a=1; session=tok; b=2", "session")
// "tok"
fn nauth_extract_cookie(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nauth.extract_cookie", span)?;
    let header = string_arg(args, 0, "nauth.extract_cookie", span)?;
    let name = string_arg(args, 1, "nauth.extract_cookie", span)?;
    match extract_cookie(&header, &name) {
        Some(v) => Ok(str_val(v)),
        None => Ok(Value::Nil.ref_cell()),
    }
}

// ---------------------------------------------------------------------------
// Auth methods
// ---------------------------------------------------------------------------

fn nauth_auth_hash_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "auth.hash_password", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.hash_password")?;
    let password = string_arg(args, 1, "auth.hash_password", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.hash_password(&password).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(h)) => Ok(str_val(h)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_verify_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "auth.verify_password", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.verify_password")?;
    let password = string_arg(args, 1, "auth.verify_password", span)?;
    let hash = string_arg(args, 2, "auth.verify_password", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.verify_password(&password, &hash).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(b)) => Ok(bool_val(b)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_verify_and_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "auth.verify_and_update", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.verify_and_update")?;
    let password = string_arg(args, 1, "auth.verify_and_update", span)?;
    let hash = string_arg(args, 2, "auth.verify_and_update", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.verify_and_update(&password, &hash).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(vu)) => {
            let mut m = HashMap::new();
            m.insert("ok".into(), bool_val(vu.ok));
            m.insert("updated".into(), bool_val(vu.updated));
            m.insert(
                "hash".into(),
                match vu.hash {
                    Some(h) => str_val(h),
                    None => Value::Nil.ref_cell(),
                },
            );
            Ok(Value::Object(m).ref_cell())
        }
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_login(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 5, "auth.login", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.login")?;
    let user_id = string_arg(args, 1, "auth.login", span)?;
    let password = string_arg(args, 2, "auth.login", span)?;
    let stored = string_arg(args, 3, "auth.login", span)?;
    let extra = parse_opts(args, 4, span)?;
    let roles = obj_string_list(&extra, "roles");
    let perms = obj_string_list(&extra, "permissions");
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a
            .login(&user_id, &password, &stored, &roles, &perms)
            .map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(lr)) => {
            let sess = register_session(id, lr.session);
            let mut m = HashMap::new();
            m.insert("ok".into(), bool_val(true));
            m.insert("session".into(), sess);
            m.insert("updated".into(), bool_val(lr.updated));
            m.insert(
                "hash".into(),
                match lr.hash {
                    Some(h) => str_val(h),
                    None => Value::Nil.ref_cell(),
                },
            );
            Ok(Value::Object(m).ref_cell())
        }
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_login_user(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "auth.login_user", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.login_user")?;
    let user_id = string_arg(args, 1, "auth.login_user", span)?;
    let extra = parse_opts(args, 2, span)?;
    let roles = obj_string_list(&extra, "roles");
    let perms = obj_string_list(&extra, "permissions");
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a
            .login_user(&user_id, &roles, &perms)
            .map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(s)) => Ok(register_session(id, s)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_logout(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "auth.logout", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.logout")?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => Ok(a.logout_cookie()),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_load_session(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "auth.load_session", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.load_session")?;
    let token = string_arg(args, 1, "auth.load_session", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.load_session(&token).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(s)) => Ok(register_session(id, s)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_session_from_cookie(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "auth.session_from_cookie", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.session_from_cookie")?;
    let header = string_arg(args, 1, "auth.session_from_cookie", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.session_from_cookie(&header).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(Some(s))) => Ok(register_session(id, s)),
        Ok(Ok(None)) => Ok(Value::Nil.ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_cookie(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "auth.cookie", span)?;
    let auth_id = handle_id_from_arg(args, 0, span, "auth.cookie")?;
    let sess_id = handle_id_from_arg(args, 1, span, "auth.cookie")?;
    let data = match with_handle(sess_id, span, |h| match h {
        NauthHandle::Session { data, .. } => Ok(data.clone()),
        _ => Err(nauth_err(span, "expected session handle")),
    })? {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => return Ok(e),
        Err(e) => return Ok(e),
    };
    match with_handle(auth_id, span, |h| match h {
        NauthHandle::Auth(a) => a.cookie_header(&data).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(s)) => Ok(str_val(s)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_reset_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "auth.reset_token", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.reset_token")?;
    let user_id = string_arg(args, 1, "auth.reset_token", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.reset_token(&user_id).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(t)) => Ok(str_val(t)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_verify_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "auth.verify_reset", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.verify_reset")?;
    let token = string_arg(args, 1, "auth.verify_reset", span)?;
    let max_age = if args.len() >= 3 {
        Some(optional_int(args, 2, DEFAULT_RESET_MAX_AGE as i64).max(0) as u64)
    } else {
        None
    };
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.verify_reset(&token, max_age).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(uid)) => Ok(str_val(uid)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_complete_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "auth.complete_reset", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.complete_reset")?;
    let token = string_arg(args, 1, "auth.complete_reset", span)?;
    let new_pw = string_arg(args, 2, "auth.complete_reset", span)?;
    let max_age = if args.len() >= 4 {
        Some(optional_int(args, 3, DEFAULT_RESET_MAX_AGE as i64).max(0) as u64)
    } else {
        None
    };
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a
            .complete_reset(&token, &new_pw, max_age)
            .map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(r)) => {
            let mut m = HashMap::new();
            m.insert("user_id".into(), str_val(r.user_id));
            m.insert("hash".into(), str_val(r.hash));
            Ok(Value::Object(m).ref_cell())
        }
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_csrf_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "auth.csrf_token", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.csrf_token")?;
    let sid = match sid_from_arg(args, 1, span, "auth.csrf_token")? {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => a.csrf_token(&sid).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(t)) => Ok(str_val(t)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_validate_csrf(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "auth.validate_csrf", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.validate_csrf")?;
    let token = string_arg(args, 1, "auth.validate_csrf", span)?;
    let sid = match sid_from_arg(args, 2, span, "auth.validate_csrf")? {
        Ok(s) => s,
        Err(e) => return Ok(e),
    };
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => Ok(a.validate_csrf(&sid, &token)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(b)) => Ok(bool_val(b)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_allows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "auth.allows", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.allows")?;
    let roles = string_array_arg(args, 1, "auth.allows", span)?;
    let required = string_arg(args, 2, "auth.allows", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => Ok(a.allows(&roles, &required)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(b)) => Ok(bool_val(b)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_expand_roles(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "auth.expand_roles", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.expand_roles")?;
    let roles = string_array_arg(args, 1, "auth.expand_roles", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => Ok(a.expand_roles(&roles)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(r)) => Ok(string_array_val(&r)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_auth_has_permission(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "auth.has_permission", span)?;
    let id = handle_id_from_arg(args, 0, span, "auth.has_permission")?;
    let perms = string_array_arg(args, 1, "auth.has_permission", span)?;
    let perm = string_arg(args, 2, "auth.has_permission", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Auth(a) => Ok(a.has_permission(&perms, &perm)),
        _ => Err(nauth_err(span, "invalid auth handle")),
    })? {
        Ok(Ok(b)) => Ok(bool_val(b)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Session methods
// ---------------------------------------------------------------------------

fn nauth_session_token(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "session.token", span)?;
    let id = handle_id_from_arg(args, 0, span, "session.token")?;
    let (auth_id, data) = match with_handle(id, span, |h| match h {
        NauthHandle::Session { auth_id, data } => Ok((*auth_id, data.clone())),
        _ => Err(nauth_err(span, "invalid session handle")),
    })? {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return Ok(e),
        Err(e) => return Ok(e),
    };
    match with_handle(auth_id, span, |h| match h {
        NauthHandle::Auth(a) => a.sign_session(&data).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "parent auth handle missing")),
    })? {
        Ok(Ok(t)) => Ok(str_val(t)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_session_cookie_method(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "session.cookie", span)?;
    let id = handle_id_from_arg(args, 0, span, "session.cookie")?;
    let (auth_id, data) = match with_handle(id, span, |h| match h {
        NauthHandle::Session { auth_id, data } => Ok((*auth_id, data.clone())),
        _ => Err(nauth_err(span, "invalid session handle")),
    })? {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return Ok(e),
        Err(e) => return Ok(e),
    };
    match with_handle(auth_id, span, |h| match h {
        NauthHandle::Auth(a) => a.cookie_header(&data).map_err(|e| map_err(span, e)),
        _ => Err(nauth_err(span, "parent auth handle missing")),
    })? {
        Ok(Ok(t)) => Ok(str_val(t)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_session_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "session.get", span)?;
    let id = handle_id_from_arg(args, 0, span, "session.get")?;
    let key = string_arg(args, 1, "session.get", span)?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Session { data, .. } => Ok(data.get(&key).map(json_to_niao)),
        _ => Err(nauth_err(span, "invalid session handle")),
    })? {
        Ok(Ok(Some(v))) => Ok(v),
        Ok(Ok(None)) => Ok(Value::Nil.ref_cell()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_session_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "session.set", span)?;
    let id = handle_id_from_arg(args, 0, span, "session.set")?;
    let key = string_arg(args, 1, "session.set", span)?;
    let json = niao_to_json(&args[2].borrow());
    match with_handle(id, span, |h| match h {
        NauthHandle::Session { data, .. } => {
            data.set(key, json);
            Ok(())
        }
        _ => Err(nauth_err(span, "invalid session handle")),
    })? {
        Ok(Ok(())) => Ok(args[0].clone()),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_session_refresh(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "session.refresh", span)?;
    let id = handle_id_from_arg(args, 0, span, "session.refresh")?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Session { auth_id, data } => Ok((*auth_id, data.clone())),
        _ => Err(nauth_err(span, "invalid session handle")),
    })? {
        Ok(Ok((auth_id, data))) => Ok(register_session(auth_id, data)),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

fn nauth_session_to_object(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "session.to_object", span)?;
    let id = handle_id_from_arg(args, 0, span, "session.to_object")?;
    match with_handle(id, span, |h| match h {
        NauthHandle::Session { data, .. } => {
            let mut m = HashMap::new();
            m.insert("user_id".into(), str_val(data.user_id.clone()));
            m.insert("session_id".into(), str_val(data.session_id.clone()));
            m.insert("roles".into(), string_array_val(&data.roles));
            m.insert("permissions".into(), string_array_val(&data.permissions));
            m.insert("data".into(), json_to_niao(&JsonValue::object(data.data.clone())));
            m.insert("is_authenticated".into(), bool_val(true));
            Ok(Value::Object(m).ref_cell())
        }
        _ => Err(nauth_err(span, "invalid session handle")),
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nauth_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nauth_fns![
    ("nauth_auth", "auth", nauth_auth),
    ("nauth_hash_password", "hash_password", nauth_hash_password),
    ("nauth_verify_password", "verify_password", nauth_verify_password),
    ("nauth_verify_and_update", "verify_and_update", nauth_verify_and_update),
    ("nauth_user", "user", nauth_user),
    ("nauth_anonymous", "anonymous", nauth_anonymous),
    ("nauth_compare", "compare", nauth_compare),
    ("nauth_token", "token", nauth_token),
    ("nauth_has_role", "has_role", nauth_has_role),
    ("nauth_has_permission", "has_permission", nauth_has_permission),
    ("nauth_roles_expand", "roles_expand", nauth_roles_expand),
    ("nauth_roles_allows", "roles_allows", nauth_roles_allows),
    ("nauth_extract_cookie", "extract_cookie", nauth_extract_cookie),
];

pub const MODULE_NAME: &str = "nauth";
pub const MODULE_PATHS: &[&str] = &["nauth", "std/nauth"];

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
    map.insert(
        "DEFAULT_SESSION_LIFETIME".to_string(),
        int_val(DEFAULT_SESSION_LIFETIME as i64),
    );
    map.insert(
        "DEFAULT_COOKIE_NAME".to_string(),
        str_val(DEFAULT_COOKIE_NAME),
    );
    map.insert(
        "DEFAULT_RESET_MAX_AGE".to_string(),
        int_val(DEFAULT_RESET_MAX_AGE as i64),
    );
    map.insert(
        "DEFAULT_TOKEN_BYTES".to_string(),
        int_val(DEFAULT_TOKEN_BYTES as i64),
    );
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn auth_session_roundtrip() {
        let auth = nauth_auth(
            &[
                Value::String("test-secret-key-32-bytes-long!!!!".into()).ref_cell(),
                {
                    let mut o = HashMap::new();
                    o.insert("scheme".into(), str_val("bcrypt"));
                    o.insert("bcrypt_cost".into(), int_val(4));
                    Value::Object(o).ref_cell()
                },
            ],
            span(),
        )
        .unwrap();
        let auth_id = match &*auth.borrow() {
            Value::Object(m) => match &*m.get("id").unwrap().borrow() {
                Value::Int(n) => *n,
                _ => panic!("no id"),
            },
            _ => panic!("expected object"),
        };
        let sess = nauth_auth_login_user(
            &[
                auth.clone(),
                Value::String("alice".into()).ref_cell(),
                {
                    let mut o = HashMap::new();
                    o.insert(
                        "roles".into(),
                        Value::Array(vec![str_val("admin")]).ref_cell(),
                    );
                    Value::Object(o).ref_cell()
                },
            ],
            span(),
        )
        .unwrap();
        let tok = nauth_session_token(&[sess.clone()], span()).unwrap();
        match &*tok.borrow() {
            Value::String(s) => assert!(!s.is_empty()),
            other => panic!("expected token string, got {other:?}"),
        }
        let _ = auth_id;
    }
}
