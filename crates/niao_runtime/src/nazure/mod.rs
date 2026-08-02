//! Native `nazure` standard library — lightweight Azure helper for Niao.
//!
//! Covers:
//! - **Blob Storage** REST (PUT, GET, DELETE, LIST)
//! - **Table Storage** REST / OData (INSERT, QUERY, DELETE)
//! - **Azure Functions** HTTP trigger (INVOKE)
//!
//! Authentication: SharedKey HMAC (Blob), SharedKeyLite HMAC (Table),
//! optional OAuth 2.0 Bearer token (client credentials), or SAS token.
//!
//! Import with `import "nazure"` or `import "std/nazure"`.

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

mod auth;
mod blob;
mod functions;
mod table;

// ──────────────────────────────────────────────────────────────────────────────
// Azure config — stored in thread-local handle registry
// ──────────────────────────────────────────────────────────────────────────────

/// Configuration for an Azure account connection.
pub(super) struct AzureConfig {
    /// Storage / Table account name.
    pub account: String,
    /// Base64-decoded storage account key (for SharedKey auth).
    pub key: Option<Vec<u8>>,
    /// SAS token string (for Blob/Table) or function key (for Functions).
    pub sas: Option<String>,
    /// Azure AD tenant ID (for Bearer token).
    pub tenant: Option<String>,
    /// Azure AD application (client) ID.
    pub client_id: Option<String>,
    /// Azure AD client secret.
    pub client_secret: Option<String>,
}

thread_local! {
    static CONFIGS: RefCell<HashMap<i64, AzureConfig>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_id() -> i64 {
    NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn with_config<T>(id: i64, span: Span, f: impl FnOnce(&AzureConfig) -> T) -> Result<T, ValueRef> {
    CONFIGS.with(|cs| {
        let cs = cs.borrow();
        match cs.get(&id) {
            Some(cfg) => Ok(f(cfg)),
            None => Err(error_value(
                codes::E2811_NAZURE_ERROR,
                "nazure_error",
                format!("invalid or closed nazure config handle {id}"),
                span,
            )),
        }
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Argument helpers
// ──────────────────────────────────────────────────────────────────────────────

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2810_NAZURE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2810_NAZURE_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
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

/// Extract a `String` field from a `Value::Object`, returning `None` if absent or non-string.
fn obj_str(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn arity_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2810_NAZURE_ARITY, "nazure_error", msg.into(), span)
}

fn type_val_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2812_NAZURE_TYPE, "nazure_error", msg.into(), span)
}

// ──────────────────────────────────────────────────────────────────────────────
// Builtin: nazure_config
// ──────────────────────────────────────────────────────────────────────────────

/// `nazure_config(opts_object) -> config_handle_int`
///
/// `opts_object` fields (all strings):
/// - `account` (required)
/// - `key` — base64-encoded storage account key (optional)
/// - `sas` — SAS token or function key (optional)
/// - `tenant`, `client_id`, `client_secret` — for Bearer auth (optional)
fn nazure_config(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nazure_config", span)?;
    let map = match &*args[0].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Ok(type_val_err(
                span,
                format!(
                    "nazure_config() expects an object, got {}",
                    other.type_name()
                ),
            ))
        }
    };

    let account = match obj_str(&map, "account") {
        Some(a) if !a.is_empty() => a,
        _ => {
            return Ok(error_value(
                codes::E2812_NAZURE_TYPE,
                "nazure_error",
                "nazure_config() missing required field 'account'",
                span,
            ))
        }
    };

    // Decode the base64 storage key, if provided.
    let key = if let Some(k) = obj_str(&map, "key") {
        match niao_codec::base64::decode_standard(&k) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                return Ok(error_value(
                    codes::E2813_NAZURE_AUTH,
                    "nazure_error",
                    format!("nazure_config() invalid base64 key: {e}"),
                    span,
                ))
            }
        }
    } else {
        None
    };

    let cfg = AzureConfig {
        account,
        key,
        sas: obj_str(&map, "sas"),
        tenant: obj_str(&map, "tenant"),
        client_id: obj_str(&map, "client_id"),
        client_secret: obj_str(&map, "client_secret"),
    };
    let id = alloc_id();
    CONFIGS.with(|cs| cs.borrow_mut().insert(id, cfg));
    Ok(Value::Int(id).ref_cell())
}

// ──────────────────────────────────────────────────────────────────────────────
// Blob builtins
// ──────────────────────────────────────────────────────────────────────────────

/// `nazure_blob_put(config_id, container, blob, body, content_type?) -> {etag, status}`
fn nazure_blob_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 5, "nazure_blob_put", span)?;
    let id = int_arg(args, 0, "nazure_blob_put", span)?;
    let container = string_arg(args, 1, "nazure_blob_put", span)?;
    let blob_name = string_arg(args, 2, "nazure_blob_put", span)?;
    let body = match &*args[3].borrow() {
        Value::String(s) => s.as_bytes().to_vec(),
        Value::ByteArray(b) => b.iter().map(|&x| x as u8).collect(),
        other => {
            return Ok(type_val_err(
                span,
                format!(
                    "nazure_blob_put() expects string or bytes for body, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    let ct = if args.len() == 5 {
        string_arg(args, 4, "nazure_blob_put", span)?
    } else {
        "application/octet-stream".to_string()
    };
    match with_config(id, span, |cfg| {
        blob::blob_put(cfg, &container, &blob_name, body.clone(), &ct, span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `nazure_blob_get(config_id, container, blob) -> {body, status}`
fn nazure_blob_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nazure_blob_get", span)?;
    let id = int_arg(args, 0, "nazure_blob_get", span)?;
    let container = string_arg(args, 1, "nazure_blob_get", span)?;
    let blob_name = string_arg(args, 2, "nazure_blob_get", span)?;
    match with_config(id, span, |cfg| {
        blob::blob_get(cfg, &container, &blob_name, span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `nazure_blob_delete(config_id, container, blob) -> true`
fn nazure_blob_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nazure_blob_delete", span)?;
    let id = int_arg(args, 0, "nazure_blob_delete", span)?;
    let container = string_arg(args, 1, "nazure_blob_delete", span)?;
    let blob_name = string_arg(args, 2, "nazure_blob_delete", span)?;
    match with_config(id, span, |cfg| {
        blob::blob_delete(cfg, &container, &blob_name, span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `nazure_blob_list(config_id, container, prefix?) -> names[]`
fn nazure_blob_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nazure_blob_list", span)?;
    let id = int_arg(args, 0, "nazure_blob_list", span)?;
    let container = string_arg(args, 1, "nazure_blob_list", span)?;
    let prefix = if args.len() == 3 {
        Some(string_arg(args, 2, "nazure_blob_list", span)?)
    } else {
        None
    };
    match with_config(id, span, |cfg| {
        blob::blob_list(cfg, &container, prefix.as_deref(), span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Table builtins
// ──────────────────────────────────────────────────────────────────────────────

/// `nazure_table_insert(config_id, table, entity) -> object`
///
/// `entity` must be a `Value::Object` map; it is JSON-serialised before sending.
fn nazure_table_insert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nazure_table_insert", span)?;
    let id = int_arg(args, 0, "nazure_table_insert", span)?;
    let table_name = string_arg(args, 1, "nazure_table_insert", span)?;
    let entity_json = value_to_json_string(&args[2].borrow(), span)?;
    match with_config(id, span, |cfg| {
        table::table_insert(cfg, &table_name, &entity_json, span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `nazure_table_query(config_id, table, filter?) -> rows[]`
fn nazure_table_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nazure_table_query", span)?;
    let id = int_arg(args, 0, "nazure_table_query", span)?;
    let table_name = string_arg(args, 1, "nazure_table_query", span)?;
    let filter = if args.len() == 3 {
        Some(string_arg(args, 2, "nazure_table_query", span)?)
    } else {
        None
    };
    match with_config(id, span, |cfg| {
        table::table_query(cfg, &table_name, filter.as_deref(), span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `nazure_table_delete(config_id, table, entity) -> true`
///
/// `entity` must contain `PartitionKey` and `RowKey` string fields.
fn nazure_table_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nazure_table_delete", span)?;
    let id = int_arg(args, 0, "nazure_table_delete", span)?;
    let table_name = string_arg(args, 1, "nazure_table_delete", span)?;
    let (pk, rk) = match &*args[2].borrow() {
        Value::Object(map) => {
            let pk = obj_str(map, "PartitionKey").unwrap_or_default();
            let rk = obj_str(map, "RowKey").unwrap_or_default();
            (pk, rk)
        }
        other => {
            return Ok(type_val_err(
                span,
                format!(
                    "nazure_table_delete() expects an entity object, got {}",
                    other.type_name()
                ),
            ))
        }
    };
    match with_config(id, span, |cfg| {
        table::table_delete(cfg, &table_name, &pk, &rk, span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Functions builtin
// ──────────────────────────────────────────────────────────────────────────────

/// `nazure_function_invoke(config_id, app, fn_name, payload) -> {status, body}`
///
/// `payload` can be a string (sent as-is) or an object (JSON-serialised).
fn nazure_function_invoke(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nazure_function_invoke", span)?;
    let id = int_arg(args, 0, "nazure_function_invoke", span)?;
    let app = string_arg(args, 1, "nazure_function_invoke", span)?;
    let fn_name = string_arg(args, 2, "nazure_function_invoke", span)?;
    let payload = match &*args[3].borrow() {
        Value::String(s) => s.clone(),
        Value::Nil => "null".to_string(),
        other => {
            // Attempt JSON serialisation.
            value_to_json_string(other, span)?
        }
    };
    match with_config(id, span, |cfg| {
        functions::function_invoke(cfg, &app, &fn_name, &payload, span)
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Minimal value → JSON string serialiser (for entity/payload encoding)
// ──────────────────────────────────────────────────────────────────────────────

fn value_to_json_string(v: &Value, span: Span) -> NiaoResult<String> {
    match v {
        Value::Nil => Ok("null".into()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(n) => Ok(n.to_string()),
        Value::Float(f) => Ok(f.to_string()),
        Value::String(s) => Ok(json_escape_string(s)),
        Value::Object(map) => {
            let mut parts = Vec::with_capacity(map.len());
            for (k, val) in map {
                let ks = json_escape_string(k);
                let vs = value_to_json_string(&val.borrow(), span)?;
                parts.push(format!("{ks}:{vs}"));
            }
            Ok(format!("{{{}}}", parts.join(",")))
        }
        Value::Array(arr) => {
            let mut parts = Vec::with_capacity(arr.len());
            for item in arr {
                parts.push(value_to_json_string(&item.borrow(), span)?);
            }
            Ok(format!("[{}]", parts.join(",")))
        }
        other => Err(RuntimeError::at(
            span,
            codes::E2812_NAZURE_TYPE,
            format!(
                "nazure: cannot JSON-serialise value of type {}",
                other.type_name()
            ),
        )),
    }
}

fn json_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ──────────────────────────────────────────────────────────────────────────────
// Registration
// ──────────────────────────────────────────────────────────────────────────────

macro_rules! nazure_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nazure_fns![
    ("nazure_config", "config", nazure_config),
    ("nazure_blob_put", "blob_put", nazure_blob_put),
    ("nazure_blob_get", "blob_get", nazure_blob_get),
    ("nazure_blob_delete", "blob_delete", nazure_blob_delete),
    ("nazure_blob_list", "blob_list", nazure_blob_list),
    ("nazure_table_insert", "table_insert", nazure_table_insert),
    ("nazure_table_query", "table_query", nazure_table_query),
    ("nazure_table_delete", "table_delete", nazure_table_delete),
    (
        "nazure_function_invoke",
        "function_invoke",
        nazure_function_invoke
    ),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nazure";
pub const MODULE_PATHS: &[&str] = &["nazure", "std/nazure"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

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

    fn obj(pairs: &[(&str, &str)]) -> ValueRef {
        let mut map = HashMap::new();
        for &(k, v) in pairs {
            map.insert(k.to_string(), Value::String(v.to_string()).ref_cell());
        }
        Value::Object(map).ref_cell()
    }

    #[test]
    fn config_missing_account_returns_error() {
        let v = nazure_config(&[obj(&[("key", "dGVzdA==")])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn config_invalid_base64_key_returns_error() {
        let v = nazure_config(
            &[obj(&[("account", "myacct"), ("key", "!not-valid-base64!")])],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn config_valid_no_key_returns_int_handle() {
        let v = nazure_config(&[obj(&[("account", "myacct")])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)));
    }

    #[test]
    fn config_valid_with_key_returns_int_handle() {
        // A valid base64 string that decodes to 32 bytes.
        let key_b64 = niao_codec::base64::encode_standard(&[0u8; 32]);
        let v = nazure_config(&[obj(&[("account", "acct"), ("key", &key_b64)])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)));
    }

    #[test]
    fn invalid_handle_blob_get_returns_error() {
        let v = nazure_blob_get(&[Value::Int(999_999).ref_cell(), s("c"), s("b")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn invalid_handle_table_insert_returns_error() {
        let entity = obj(&[("PartitionKey", "pk"), ("RowKey", "rk")]);
        let v =
            nazure_table_insert(&[Value::Int(888_888).ref_cell(), s("T"), entity], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn invalid_handle_function_invoke_returns_error() {
        let v = nazure_function_invoke(
            &[
                Value::Int(777_777).ref_cell(),
                s("myapp"),
                s("myFunc"),
                s("{}"),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn value_to_json_primitives() {
        assert_eq!(value_to_json_string(&Value::Nil, span()).unwrap(), "null");
        assert_eq!(
            value_to_json_string(&Value::Bool(true), span()).unwrap(),
            "true"
        );
        assert_eq!(value_to_json_string(&Value::Int(42), span()).unwrap(), "42");
        assert_eq!(
            value_to_json_string(&Value::String("hello".into()), span()).unwrap(),
            "\"hello\""
        );
    }

    #[test]
    fn value_to_json_string_escapes() {
        let s = Value::String("a\"b\\c\nd".into());
        let json = value_to_json_string(&s, span()).unwrap();
        assert_eq!(json, r#""a\"b\\c\nd""#);
    }

    #[test]
    fn value_to_json_object() {
        let mut map = HashMap::new();
        map.insert("x".to_string(), Value::Int(1).ref_cell());
        let v = Value::Object(map);
        let json = value_to_json_string(&v, span()).unwrap();
        assert!(json.contains("\"x\":1"));
    }

    #[test]
    fn namespace_has_all_methods() {
        let ns = namespace();
        match ns {
            Value::Object(map) => {
                for key in &[
                    "config",
                    "blob_put",
                    "blob_get",
                    "blob_delete",
                    "blob_list",
                    "table_insert",
                    "table_query",
                    "table_delete",
                    "function_invoke",
                ] {
                    assert!(map.contains_key(*key), "namespace missing: {key}");
                }
            }
            other => panic!("namespace() should return Object, got {other:?}"),
        }
    }

    #[test]
    fn builtins_count() {
        assert_eq!(builtins().len(), 9);
    }
}
