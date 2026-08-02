//! Native nkeyring standard library — OS credential stores (~Python `keyring` subset).
//!
//! macOS Keychain, Linux Secret Service, Windows Credential Manager (DPAPI-backed).
//! Import with `import "nkeyring"` (or `import "std/nkeyring"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_keyring as kr;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3578_NKEYRING_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3576_NKEYRING_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3576_NKEYRING_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nkeyring_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3577_NKEYRING_ERROR, "nkeyring_error", msg.into(), span)
}

fn nkeyring_not_found(span: Span, service: &str, user: &str) -> ValueRef {
    error_value(
        codes::E3579_NKEYRING_NOT_FOUND,
        "nkeyring_error",
        format!("credential not found: service={service}, user={user}"),
        span,
    )
}

fn nkeyring_access(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3580_NKEYRING_ACCESS, "nkeyring_error", msg.into(), span)
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

fn secret_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string or bytes as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn service_user(args: &[ValueRef], name: &str, span: Span) -> NiaoResult<(String, String)> {
    Ok((string_arg(args, 0, name, span)?, string_arg(args, 1, name, span)?))
}

fn map_keyring_err(span: Span, e: kr::KeyringError) -> ValueRef {
    match e {
        kr::KeyringError::NotFound => nkeyring_not_found(span, "", ""),
        kr::KeyringError::Access(_) | kr::KeyringError::Platform(_) => {
            nkeyring_access(span, e.to_string())
        }
        other => nkeyring_err(span, other.to_string()),
    }
}

fn map_delete_err(span: Span, service: &str, user: &str, e: kr::KeyringError) -> ValueRef {
    match e {
        kr::KeyringError::NotFound => nkeyring_not_found(span, service, user),
        kr::KeyringError::Access(_) | kr::KeyringError::Platform(_) => {
            nkeyring_access(span, e.to_string())
        }
        other => nkeyring_err(span, other.to_string()),
    }
}

fn credential_object(service: &str, username: &str, password: &str) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("service".to_string(), Value::String(service.to_string()).ref_cell());
    map.insert("username".to_string(), Value::String(username.to_string()).ref_cell());
    map.insert("user".to_string(), Value::String(username.to_string()).ref_cell());
    map.insert("password".to_string(), Value::String(password.to_string()).ref_cell());
    Value::Object(map).ref_cell()
}

fn entry_object(service: &str, user: &str) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("service".to_string(), Value::String(service.to_string()).ref_cell());
    map.insert("user".to_string(), Value::String(user.to_string()).ref_cell());
    map.insert("username".to_string(), Value::String(user.to_string()).ref_cell());
    map.insert("kind".to_string(), Value::String("entry".into()).ref_cell());
    Value::Object(map).ref_cell()
}

fn entry_from_arg(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<(String, String)> {
    if args.len() >= 2 {
        return service_user(args, "entry", span);
    }
    match &*args[idx].borrow() {
        Value::Object(map) => {
            let service = map
                .get("service")
                .and_then(|v| match &*v.borrow() {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .ok_or_else(|| type_err(span, "entry object requires service field"))?;
            let user = map
                .get("user")
                .or_else(|| map.get("username"))
                .and_then(|v| match &*v.borrow() {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .ok_or_else(|| type_err(span, "entry object requires user/username field"))?;
            Ok((service, user))
        }
        other => Err(type_err(
            span,
            format!(
                "argument {} must be entry object or (service, user) strings, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Module-level API (~Python keyring)
// ---------------------------------------------------------------------------

/// nkeyring_get_password(service, user) → string | nil
///
/// >>> nkeyring.get_password("svc", "alice")
/// => nil
fn nkeyring_get_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_get_password", span)?;
    let (service, user) = service_user(args, "nkeyring_get_password", span)?;
    match kr::get_password(&service, &user) {
        Ok(Some(p)) => Ok(Value::String(p).ref_cell()),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_set_password(service, user, password) → nil
///
/// >>> nkeyring.set_password("svc", "alice", "secret")
/// => nil
fn nkeyring_set_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nkeyring_set_password", span)?;
    let service = string_arg(args, 0, "nkeyring_set_password", span)?;
    let user = string_arg(args, 1, "nkeyring_set_password", span)?;
    let password = string_arg(args, 2, "nkeyring_set_password", span)?;
    match kr::set_password(&service, &user, &password) {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_delete_password(service, user) → nil
///
/// >>> nkeyring.delete_password("svc", "alice")
/// => nil
fn nkeyring_delete_password(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_delete_password", span)?;
    let (service, user) = service_user(args, "nkeyring_delete_password", span)?;
    match kr::delete_password(&service, &user) {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_delete_err(span, &service, &user, e)),
    }
}

/// nkeyring_get_secret(service, user) → bytes | nil
fn nkeyring_get_secret(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_get_secret", span)?;
    let (service, user) = service_user(args, "nkeyring_get_secret", span)?;
    match kr::get_secret(&service, &user) {
        Ok(Some(b)) => Ok(Value::ByteArray(b).ref_cell()),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_set_secret(service, user, secret) → nil
fn nkeyring_set_secret(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nkeyring_set_secret", span)?;
    let service = string_arg(args, 0, "nkeyring_set_secret", span)?;
    let user = string_arg(args, 1, "nkeyring_set_secret", span)?;
    let secret = secret_arg(args, 2, "nkeyring_set_secret", span)?;
    match kr::set_secret(&service, &user, &secret) {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_get_credential(service, user) → {service, username, password} | nil
///
/// >>> nkeyring.get_credential("svc", "alice")
/// => nil
fn nkeyring_get_credential(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_get_credential", span)?;
    let (service, user) = service_user(args, "nkeyring_get_credential", span)?;
    match kr::get_credential(&service, &user) {
        Ok(Some(c)) => Ok(credential_object(&c.service, &c.username, &c.password)),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_exists(service, user) → bool
///
/// >>> nkeyring.exists("svc", "alice")
/// => false
fn nkeyring_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_exists", span)?;
    let (service, user) = service_user(args, "nkeyring_exists", span)?;
    match kr::exists(&service, &user) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_entry(service, user) → entry object
///
/// >>> let e = nkeyring.entry("svc", "bob")
/// >>> e.service
/// => "svc"
fn nkeyring_entry(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_entry", span)?;
    let (service, user) = service_user(args, "nkeyring_entry", span)?;
    Ok(entry_object(&service, &user))
}

/// nkeyring_get(entry) → string | nil — password for entry object.
fn nkeyring_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkeyring_get", span)?;
    let (service, user) = entry_from_arg(args, 0, span)?;
    match kr::get_password(&service, &user) {
        Ok(Some(p)) => Ok(Value::String(p).ref_cell()),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_set(entry, password) → nil
fn nkeyring_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_set", span)?;
    let (service, user) = entry_from_arg(args, 0, span)?;
    let password = string_arg(args, 1, "nkeyring_set", span)?;
    match kr::set_password(&service, &user, &password) {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_delete(entry) → nil
fn nkeyring_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkeyring_delete", span)?;
    let (service, user) = entry_from_arg(args, 0, span)?;
    match kr::delete_password(&service, &user) {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_delete_err(span, &service, &user, e)),
    }
}

/// nkeyring_get_bytes(entry) → bytes | nil
fn nkeyring_get_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nkeyring_get_bytes", span)?;
    let (service, user) = entry_from_arg(args, 0, span)?;
    match kr::get_secret(&service, &user) {
        Ok(Some(b)) => Ok(Value::ByteArray(b).ref_cell()),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_set_bytes(entry, secret) → nil
fn nkeyring_set_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nkeyring_set_bytes", span)?;
    let (service, user) = entry_from_arg(args, 0, span)?;
    let secret = secret_arg(args, 1, "nkeyring_set_bytes", span)?;
    match kr::set_secret(&service, &user, &secret) {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(map_keyring_err(span, e)),
    }
}

/// nkeyring_backend() → string
///
/// >>> nkeyring.backend()
/// => "windows_credential_manager"
fn nkeyring_backend(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nkeyring_backend", span)?;
    Ok(Value::String(kr::backend_name()).ref_cell())
}

/// nkeyring_platform() → string
fn nkeyring_platform(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nkeyring_platform", span)?;
    Ok(Value::String(kr::platform_name().into()).ref_cell())
}

/// nkeyring_use_memory() → nil — in-memory backend for tests (~set_keyring mock).
fn nkeyring_use_memory(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nkeyring_use_memory", span)?;
    kr::use_memory();
    Ok(Value::Nil.ref_cell())
}

/// nkeyring_use_system() → nil — restore OS credential store.
fn nkeyring_use_system(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nkeyring_use_system", span)?;
    kr::use_system();
    Ok(Value::Nil.ref_cell())
}

/// nkeyring_clear_memory() → nil — wipe in-memory store (tests).
fn nkeyring_clear_memory(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nkeyring_clear_memory", span)?;
    kr::clear_memory();
    Ok(Value::Nil.ref_cell())
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nkeyring_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nkeyring_fns![
    ("nkeyring_get_password", "get_password", nkeyring_get_password),
    ("nkeyring_set_password", "set_password", nkeyring_set_password),
    ("nkeyring_delete_password", "delete_password", nkeyring_delete_password),
    ("nkeyring_get_secret", "get_secret", nkeyring_get_secret),
    ("nkeyring_set_secret", "set_secret", nkeyring_set_secret),
    ("nkeyring_get_credential", "get_credential", nkeyring_get_credential),
    ("nkeyring_exists", "exists", nkeyring_exists),
    ("nkeyring_entry", "entry", nkeyring_entry),
    ("nkeyring_get", "get", nkeyring_get),
    ("nkeyring_set", "set", nkeyring_set),
    ("nkeyring_delete", "delete", nkeyring_delete),
    ("nkeyring_get_bytes", "get_bytes", nkeyring_get_bytes),
    ("nkeyring_set_bytes", "set_bytes", nkeyring_set_bytes),
    ("nkeyring_backend", "backend", nkeyring_backend),
    ("nkeyring_platform", "platform", nkeyring_platform),
    ("nkeyring_use_memory", "use_memory", nkeyring_use_memory),
    ("nkeyring_use_system", "use_system", nkeyring_use_system),
    ("nkeyring_clear_memory", "clear_memory", nkeyring_clear_memory),
];

pub const MODULE_NAME: &str = "nkeyring";
pub const MODULE_PATHS: &[&str] = &["nkeyring", "std/nkeyring"];

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
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn setup_mem() {
        kr::use_memory();
        kr::clear_memory();
    }

    #[test]
    fn password_roundtrip() {
        setup_mem();
        nkeyring_set_password(&[s("svc"), s("u"), s("pw")], span()).unwrap();
        let got = nkeyring_get_password(&[s("svc"), s("u")], span()).unwrap();
        assert!(matches!(&*got.borrow(), Value::String(p) if p == "pw"));
        nkeyring_delete_password(&[s("svc"), s("u")], span()).unwrap();
    }

    #[test]
    fn delete_missing_errors() {
        setup_mem();
        let e = nkeyring_delete_password(&[s("nope"), s("nope")], span()).unwrap();
        assert!(matches!(&*e.borrow(), Value::Error(_)));
    }
}
