//! Native `naws` standard library — AWS helper (S3, DynamoDB, Lambda, SSM).
//!
//! Uses SigV4 signing (HMAC-SHA256 via `niao_crypto`) and the built-in HTTP
//! client (`niao_http`). Zero new third-party crates.
//!
//! Import: `import "naws"` or `import "std/naws"`.

pub mod sigv4;
mod s3;
mod dynamodb;
mod lambda;
mod ssm;

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ── module metadata ───────────────────────────────────────────────────────────

pub const MODULE_NAME: &str = "naws";
pub const MODULE_PATHS: &[&str] = &["naws", "std/naws"];

// ── config handle registry ────────────────────────────────────────────────────

/// AWS credentials and region, keyed by handle id.
#[derive(Clone)]
pub(super) struct AwsConfig {
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

thread_local! {
    static CONFIGS: RefCell<HashMap<i64, AwsConfig>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_config(cfg: AwsConfig) -> i64 {
    NEXT_ID.with(|n| {
        let id = *n.borrow();
        *n.borrow_mut() = id + 1;
        CONFIGS.with(|c| c.borrow_mut().insert(id, cfg));
        id
    })
}

pub(super) fn get_config(id: i64, span: Span) -> NiaoResult<AwsConfig> {
    CONFIGS.with(|c| {
        c.borrow()
            .get(&id)
            .cloned()
            .ok_or_else(|| RuntimeError::at(span, codes::E2803_NAWS_AUTH,
                format!("naws: invalid config handle {id}")))
    })
}

// ── shared helpers ────────────────────────────────────────────────────────────

pub(super) type AwsResult = NiaoResult<ValueRef>;

pub(super) fn ok_string(s: String) -> ValueRef {
    Value::String(s).ref_cell()
}

pub(super) fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

pub(super) fn ok_value(v: Value) -> ValueRef {
    v.ref_cell()
}

pub(super) fn aws_error(code: u32, kind: &str, msg: impl Into<String>, span: Span) -> ValueRef {
    error_value(code, kind, msg, span)
}

/// JSON-escape a string (handles `"`, `\`, control chars).
pub(super) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ── argument helpers ──────────────────────────────────────────────────────────

pub(super) fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(RuntimeError::TypeError {
            message: format!("{name}() expects int at arg {}, got {}", idx + 1, other.type_name()),
            line: span.line,
            col: span.col,
        }),
    }
}

pub(super) fn str_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::TypeError {
            message: format!("{name}() expects string at arg {}, got {}", idx + 1, other.type_name()),
            line: span.line,
            col: span.col,
        }),
    }
}

pub(super) fn obj_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(m) => Ok(m.clone()),
        other => Err(RuntimeError::TypeError {
            message: format!("{name}() expects object at arg {}, got {}", idx + 1, other.type_name()),
            line: span.line,
            col: span.col,
        }),
    }
}

pub(super) fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.iter().map(|&x| x as u8).collect()),
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                match &*v.borrow() {
                    Value::Int(n) => out.push(*n as u8),
                    _ => return Err(RuntimeError::TypeError {
                        message: format!("{name}() byte array must contain integers"),
                        line: span.line, col: span.col,
                    }),
                }
            }
            Ok(out)
        }
        other => Err(RuntimeError::TypeError {
            message: format!("{name}() expects string or bytes at arg {}, got {}", idx + 1, other.type_name()),
            line: span.line,
            col: span.col,
        }),
    }
}

pub(super) fn string_opt(args: &[ValueRef], idx: usize) -> Option<String> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    })
}

// ── naws.config ───────────────────────────────────────────────────────────────

/// `naws.config({region, access_key, secret_key, session_token?}) → config_id`
fn naws_config(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_config() expects 1 argument: {region, access_key, secret_key, session_token?}",
        ));
    }
    let map = obj_arg(args, 0, "naws_config", span)?;

    let region = map
        .get("region")
        .and_then(|v| match &*v.borrow() { Value::String(s) => Some(s.clone()), _ => None })
        .ok_or_else(|| RuntimeError::at(span, codes::E2802_NAWS_TYPE,
            "naws_config: missing required field 'region'"))?;
    let access_key = map
        .get("access_key")
        .and_then(|v| match &*v.borrow() { Value::String(s) => Some(s.clone()), _ => None })
        .ok_or_else(|| RuntimeError::at(span, codes::E2802_NAWS_TYPE,
            "naws_config: missing required field 'access_key'"))?;
    let secret_key = map
        .get("secret_key")
        .and_then(|v| match &*v.borrow() { Value::String(s) => Some(s.clone()), _ => None })
        .ok_or_else(|| RuntimeError::at(span, codes::E2802_NAWS_TYPE,
            "naws_config: missing required field 'secret_key'"))?;
    let session_token = map
        .get("session_token")
        .and_then(|v| match &*v.borrow() { Value::String(s) => Some(s.clone()), _ => None });

    let id = alloc_config(AwsConfig { region, access_key, secret_key, session_token });
    Ok(Value::Int(id).ref_cell())
}

// ── flat builtin wrappers (for builtin_table registration) ────────────────────

fn naws_s3_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    s3::s3_put(args, span)
}
fn naws_s3_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    s3::s3_get(args, span)
}
fn naws_s3_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    s3::s3_delete(args, span)
}
fn naws_s3_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    s3::s3_list(args, span)
}
fn naws_dynamodb_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    dynamodb::dynamodb_put(args, span)
}
fn naws_dynamodb_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    dynamodb::dynamodb_get(args, span)
}
fn naws_dynamodb_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    dynamodb::dynamodb_delete(args, span)
}
fn naws_dynamodb_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    dynamodb::dynamodb_query(args, span)
}
fn naws_lambda_invoke(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    lambda::lambda_invoke(args, span)
}
fn naws_ssm_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    ssm::ssm_get(args, span)
}

// ── namespace object ──────────────────────────────────────────────────────────

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "config",           Rc::new(naws_config));
    bind(&mut map, "s3_put",           Rc::new(naws_s3_put));
    bind(&mut map, "s3_get",           Rc::new(naws_s3_get));
    bind(&mut map, "s3_delete",        Rc::new(naws_s3_delete));
    bind(&mut map, "s3_list",          Rc::new(naws_s3_list));
    bind(&mut map, "dynamodb_put",     Rc::new(naws_dynamodb_put));
    bind(&mut map, "dynamodb_get",     Rc::new(naws_dynamodb_get));
    bind(&mut map, "dynamodb_delete",  Rc::new(naws_dynamodb_delete));
    bind(&mut map, "dynamodb_query",   Rc::new(naws_dynamodb_query));
    bind(&mut map, "lambda_invoke",    Rc::new(naws_lambda_invoke));
    bind(&mut map, "ssm_get",          Rc::new(naws_ssm_get));
    Value::Object(map)
}

// ── flat builtins (global name registration) ──────────────────────────────────

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("naws_config",          Rc::new(naws_config)),
        ("naws_s3_put",          Rc::new(naws_s3_put)),
        ("naws_s3_get",          Rc::new(naws_s3_get)),
        ("naws_s3_delete",       Rc::new(naws_s3_delete)),
        ("naws_s3_list",         Rc::new(naws_s3_list)),
        ("naws_dynamodb_put",    Rc::new(naws_dynamodb_put)),
        ("naws_dynamodb_get",    Rc::new(naws_dynamodb_get)),
        ("naws_dynamodb_delete", Rc::new(naws_dynamodb_delete)),
        ("naws_dynamodb_query",  Rc::new(naws_dynamodb_query)),
        ("naws_lambda_invoke",   Rc::new(naws_lambda_invoke)),
        ("naws_ssm_get",         Rc::new(naws_ssm_get)),
    ]
}

// ── module-level tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span { Span::dummy() }

    fn s(v: &str) -> ValueRef { Value::String(v.to_string()).ref_cell() }
    fn config_obj(region: &str, ak: &str, sk: &str) -> ValueRef {
        let mut map = HashMap::new();
        map.insert("region".into(), s(region));
        map.insert("access_key".into(), s(ak));
        map.insert("secret_key".into(), s(sk));
        Value::Object(map).ref_cell()
    }

    #[test]
    fn config_creates_handle() {
        let h = naws_config(&[config_obj("us-east-1", "AKID", "SECRET")], span()).unwrap();
        assert!(matches!(&*h.borrow(), Value::Int(n) if *n >= 1));
    }

    #[test]
    fn config_missing_region_errors() {
        let mut map = HashMap::new();
        map.insert("access_key".into(), s("AKID"));
        map.insert("secret_key".into(), s("SECRET"));
        let obj = Value::Object(map).ref_cell();
        assert!(naws_config(&[obj], span()).is_err());
    }

    #[test]
    fn config_invalid_handle_returns_error() {
        let bad_id = Value::Int(999_999).ref_cell();
        // get_config should fail with RuntimeError
        assert!(get_config(999_999, span()).is_err());
    }

    #[test]
    fn json_escape_special_chars() {
        assert_eq!(json_escape("a\"b\\c"), r#"a\"b\\c"#);
        assert_eq!(json_escape("line\nnewline"), "line\\nnewline");
    }

    #[test]
    fn namespace_has_all_keys() {
        let ns = namespace();
        let Value::Object(map) = ns else { panic!("expected object"); };
        for key in &["config", "s3_put", "s3_get", "s3_delete", "s3_list",
                     "dynamodb_put", "dynamodb_get", "dynamodb_delete", "dynamodb_query",
                     "lambda_invoke", "ssm_get"] {
            assert!(map.contains_key(*key), "missing key: {key}");
        }
    }

    #[test]
    fn builtins_count() {
        assert_eq!(builtins().len(), 11);
    }
}
