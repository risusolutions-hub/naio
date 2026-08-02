//! Native `ngcp` standard library — Google Cloud helper (GCS, Pub/Sub, Firestore, Functions).
//!
//! Auth: Bearer access token, or service-account JWT (RS256) → OAuth2 token exchange.
//! HTTP via `niao_http`; RSA via existing `rsa` dep; base64 via `niao_codec`.
//!
//! Import: `import "ngcp"` or `import "std/ngcp"`.

pub mod auth;
mod firestore;
mod functions;
mod gcs;
mod pubsub;

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const MODULE_NAME: &str = "ngcp";
pub const MODULE_PATHS: &[&str] = &["ngcp", "std/ngcp"];

// ── config handle registry ────────────────────────────────────────────────────

/// Google Cloud credentials / project, keyed by handle id.
pub(super) struct GcpConfig {
    pub project: String,
    pub client_email: Option<String>,
    pub private_key: Option<String>,
    pub access_token: Option<String>,
    pub token_uri: String,
    pub scope: String,
    /// Cached (token, expiry_unix_secs).
    pub cached_token: Option<(String, u64)>,
}

thread_local! {
    static CONFIGS: RefCell<HashMap<i64, GcpConfig>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc_config(cfg: GcpConfig) -> i64 {
    NEXT_ID.with(|n| {
        let id = *n.borrow();
        *n.borrow_mut() = id + 1;
        CONFIGS.with(|c| c.borrow_mut().insert(id, cfg));
        id
    })
}

pub(super) fn with_config_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut GcpConfig) -> T,
) -> Result<T, ValueRef> {
    CONFIGS.with(|cs| {
        let mut cs = cs.borrow_mut();
        match cs.get_mut(&id) {
            Some(cfg) => Ok(f(cfg)),
            None => Err(error_value(
                codes::E4543_NGCP_AUTH,
                "ngcp_error",
                format!("invalid or closed ngcp config handle {id}"),
                span,
            )),
        }
    })
}

pub(super) fn bearer_auth(cfg: &mut GcpConfig) -> Result<String, String> {
    let (token, new_cache) = auth::obtain_access_token(
        &cfg.access_token,
        &cfg.cached_token,
        &cfg.client_email,
        &cfg.private_key,
        &cfg.token_uri,
        &cfg.scope,
    )?;
    if let Some(c) = new_cache {
        cfg.cached_token = Some(c);
    }
    Ok(token)
}

// ── shared helpers ────────────────────────────────────────────────────────────

pub(super) type GcpResult = NiaoResult<ValueRef>;

pub(super) fn ok_string(s: String) -> ValueRef {
    Value::String(s).ref_cell()
}

pub(super) fn ok_value(v: Value) -> ValueRef {
    v.ref_cell()
}

pub(super) fn gcp_error(code: u32, kind: &str, msg: impl Into<String>, span: Span) -> ValueRef {
    error_value(code, kind, msg, span)
}

pub(super) fn type_error(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E4542_NGCP_TYPE, msg.into())
}

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

pub(super) fn value_to_json_string(v: &Value, span: Span) -> NiaoResult<String> {
    match v {
        Value::Nil => Ok("null".into()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(n) => Ok(n.to_string()),
        Value::Float(f) => Ok(f.to_string()),
        Value::String(s) => Ok(format!("\"{}\"", json_escape(s))),
        Value::Object(map) => {
            let mut parts = Vec::with_capacity(map.len());
            for (k, val) in map {
                let vs = value_to_json_string(&val.borrow(), span)?;
                parts.push(format!("\"{}\":{}", json_escape(k), vs));
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
            codes::E4542_NGCP_TYPE,
            format!("ngcp: cannot JSON-serialise value of type {}", other.type_name()),
        )),
    }
}

pub(super) fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_error(
            span,
            format!(
                "{name}() expects int at arg {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

pub(super) fn str_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_error(
            span,
            format!(
                "{name}() expects string at arg {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
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
        other => Err(type_error(
            span,
            format!(
                "{name}() expects object at arg {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
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
                    Value::Int(n) if (0..=255).contains(n) => out.push(*n as u8),
                    Value::Int(n) => {
                        return Err(type_error(
                            span,
                            format!("{name}() byte value out of range 0..255: {n}"),
                        ))
                    }
                    _ => {
                        return Err(type_error(span, format!("{name}() byte array must contain integers")))
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_error(
            span,
            format!(
                "{name}() expects string or bytes at arg {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

pub(super) fn string_opt(args: &[ValueRef], idx: usize) -> Option<String> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    })
}

fn obj_str(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

// ── ngcp.config ───────────────────────────────────────────────────────────────

/// `ngcp.config({project, client_email?, private_key?, access_token?, token_uri?, scopes?}) → config_id`
///
/// // >>> ngcp.config({project: "demo", access_token: "tok"}) > 0
/// // => true
fn ngcp_config(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    if args.len() != 1 {
        return Err(RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_config() expects 1 argument: {project, client_email?, private_key?, access_token?, ...}",
        ));
    }
    let map = match &*args[0].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Ok(gcp_error(
                codes::E4542_NGCP_TYPE,
                "ngcp_error",
                format!("ngcp_config() expects an object, got {}", other.type_name()),
                span,
            ));
        }
    };

    let project = match obj_str(&map, "project") {
        Some(p) if !p.is_empty() => p,
        _ => {
            return Ok(gcp_error(
                codes::E4542_NGCP_TYPE,
                "ngcp_error",
                "ngcp_config() missing required field 'project'",
                span,
            ));
        }
    };

    let private_key = obj_str(&map, "private_key").map(|s| auth::normalize_pem(&s));
    let scope = obj_str(&map, "scopes")
        .or_else(|| obj_str(&map, "scope"))
        .unwrap_or_else(|| auth::default_scope().to_string());
    let token_uri = obj_str(&map, "token_uri")
        .unwrap_or_else(|| auth::default_token_uri().to_string());

    let id = alloc_config(GcpConfig {
        project,
        client_email: obj_str(&map, "client_email"),
        private_key,
        access_token: obj_str(&map, "access_token"),
        token_uri,
        scope,
        cached_token: None,
    });
    Ok(Value::Int(id).ref_cell())
}

// ── flat wrappers ─────────────────────────────────────────────────────────────

fn ngcp_gcs_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    gcs::gcs_put(args, span)
}
fn ngcp_gcs_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    gcs::gcs_get(args, span)
}
fn ngcp_gcs_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    gcs::gcs_delete(args, span)
}
fn ngcp_gcs_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    gcs::gcs_list(args, span)
}
fn ngcp_pubsub_publish(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    pubsub::pubsub_publish(args, span)
}
fn ngcp_pubsub_pull(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    pubsub::pubsub_pull(args, span)
}
fn ngcp_pubsub_ack(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    pubsub::pubsub_ack(args, span)
}
fn ngcp_firestore_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    firestore::firestore_get(args, span)
}
fn ngcp_firestore_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    firestore::firestore_set(args, span)
}
fn ngcp_firestore_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    firestore::firestore_delete(args, span)
}
fn ngcp_firestore_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    firestore::firestore_query(args, span)
}
fn ngcp_function_invoke(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    functions::function_invoke(args, span)
}

// ── registration ──────────────────────────────────────────────────────────────

macro_rules! ngcp_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ngcp_fns![
    ("ngcp_config",           "config",           ngcp_config),
    ("ngcp_gcs_put",          "gcs_put",          ngcp_gcs_put),
    ("ngcp_gcs_get",          "gcs_get",          ngcp_gcs_get),
    ("ngcp_gcs_delete",       "gcs_delete",       ngcp_gcs_delete),
    ("ngcp_gcs_list",         "gcs_list",         ngcp_gcs_list),
    ("ngcp_pubsub_publish",   "pubsub_publish",   ngcp_pubsub_publish),
    ("ngcp_pubsub_pull",      "pubsub_pull",      ngcp_pubsub_pull),
    ("ngcp_pubsub_ack",       "pubsub_ack",       ngcp_pubsub_ack),
    ("ngcp_firestore_get",    "firestore_get",    ngcp_firestore_get),
    ("ngcp_firestore_set",    "firestore_set",    ngcp_firestore_set),
    ("ngcp_firestore_delete", "firestore_delete", ngcp_firestore_delete),
    ("ngcp_firestore_query",  "firestore_query",  ngcp_firestore_query),
    ("ngcp_function_invoke",  "function_invoke",  ngcp_function_invoke),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

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
    fn config_creates_handle() {
        let h = ngcp_config(&[obj(&[("project", "my-proj"), ("access_token", "t")])], span())
            .unwrap();
        assert!(matches!(&*h.borrow(), Value::Int(n) if *n >= 1));
    }

    #[test]
    fn config_missing_project_errors() {
        let v = ngcp_config(&[obj(&[("access_token", "t")])], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn config_arity_errors() {
        assert!(ngcp_config(&[], span()).is_err());
    }

    #[test]
    fn config_non_object_errors() {
        let v = ngcp_config(&[s("nope")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn invalid_handle_gcs_get_returns_error() {
        let v = ngcp_gcs_get(&[Value::Int(999_999).ref_cell(), s("b"), s("o")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn invalid_handle_pubsub_publish_returns_error() {
        let v = ngcp_pubsub_publish(
            &[Value::Int(888_888).ref_cell(), s("topic"), s("hi")],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn invalid_handle_firestore_get_returns_error() {
        let v = ngcp_firestore_get(
            &[Value::Int(777_777).ref_cell(), s("users"), s("u1")],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn invalid_handle_function_invoke_returns_error() {
        let v = ngcp_function_invoke(
            &[
                Value::Int(666_666).ref_cell(),
                s("https://example.com"),
                s("{}"),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn gcs_put_arity_errors() {
        assert!(ngcp_gcs_put(&[Value::Int(1).ref_cell()], span()).is_err());
    }

    #[test]
    fn json_escape_special_chars() {
        assert_eq!(json_escape("a\"b\\c"), r#"a\"b\\c"#);
        assert_eq!(json_escape("line\nnewline"), "line\\nnewline");
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
            value_to_json_string(&Value::String("hi".into()), span()).unwrap(),
            "\"hi\""
        );
    }

    #[test]
    fn value_to_json_unicode() {
        let j = value_to_json_string(&Value::String("café".into()), span()).unwrap();
        assert!(j.contains("café"));
    }

    #[test]
    fn value_to_json_empty_object_and_array() {
        assert_eq!(
            value_to_json_string(&Value::Object(HashMap::new()), span()).unwrap(),
            "{}"
        );
        assert_eq!(
            value_to_json_string(&Value::Array(Vec::new()), span()).unwrap(),
            "[]"
        );
    }

    #[test]
    fn namespace_has_all_keys() {
        let ns = namespace();
        let Value::Object(map) = ns else {
            panic!("expected object");
        };
        for key in &[
            "config",
            "gcs_put",
            "gcs_get",
            "gcs_delete",
            "gcs_list",
            "pubsub_publish",
            "pubsub_pull",
            "pubsub_ack",
            "firestore_get",
            "firestore_set",
            "firestore_delete",
            "firestore_query",
            "function_invoke",
        ] {
            assert!(map.contains_key(*key), "missing key: {key}");
        }
    }

    #[test]
    fn builtins_count() {
        assert_eq!(builtins().len(), 13);
    }

    #[test]
    fn config_with_sa_fields_ok() {
        let h = ngcp_config(
            &[obj(&[
                ("project", "p"),
                ("client_email", "a@b.c"),
                ("private_key", "-----BEGIN PRIVATE KEY-----\nX\n-----END PRIVATE KEY-----\n"),
            ])],
            span(),
        )
        .unwrap();
        assert!(matches!(&*h.borrow(), Value::Int(_)));
    }

    #[test]
    fn function_invoke_empty_url_errors() {
        let cfg = ngcp_config(&[obj(&[("project", "p"), ("access_token", "t")])], span()).unwrap();
        let id = match &*cfg.borrow() {
            Value::Int(n) => *n,
            _ => panic!(),
        };
        let v = ngcp_function_invoke(&[Value::Int(id).ref_cell(), s(""), s("{}")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn pubsub_ack_bad_ids_type() {
        let cfg = ngcp_config(&[obj(&[("project", "p"), ("access_token", "t")])], span()).unwrap();
        let id = match &*cfg.borrow() {
            Value::Int(n) => *n,
            _ => panic!(),
        };
        let v = ngcp_pubsub_ack(&[Value::Int(id).ref_cell(), s("sub"), s("not-array")], span())
            .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}

/// Release-mode micro-benchmarks for ngcp hot paths.
/// Run: `cargo test -p niao_runtime --release --lib ngcp::benches -- --ignored --nocapture`
#[cfg(test)]
mod benches {
    use super::*;
    use std::time::Instant;

    fn bench(name: &str, iters: u32, mut f: impl FnMut()) {
        for _ in 0..10 {
            f();
        }
        let t0 = Instant::now();
        for _ in 0..iters {
            f();
        }
        let mean_ns = t0.elapsed().as_nanos() / iters as u128;
        let ops = 1_000_000_000u128 / mean_ns.max(1);
        println!("{name}: mean={mean_ns} ns/op  ~{ops} ops/sec  (n={iters})");
    }

    #[test]
    #[ignore]
    fn ngcp_hot_paths() {
        let span = Span::dummy();
        let mut map = HashMap::new();
        map.insert("project".into(), Value::String("bench".into()).ref_cell());
        map.insert("access_token".into(), Value::String("tok".into()).ref_cell());
        let opts = Value::Object(map).ref_cell();

        bench("config", 5_000, || {
            let _ = ngcp_config(std::slice::from_ref(&opts), span).unwrap();
        });

        bench("json_escape_1k", 10_000, || {
            let s = "x".repeat(1024);
            let _ = json_escape(&s);
        });

        bench("uri_encode_path", 20_000, || {
            let _ = auth::uri_encode_path("folder/file name.txt");
        });

        bench("firestore_encode_fields", 10_000, || {
            let mut m = HashMap::new();
            m.insert("name".into(), Value::String("Ada".into()).ref_cell());
            m.insert("age".into(), Value::Int(36).ref_cell());
            let _ = firestore::encode_fields(&m);
        });

        // Naive baseline: manual string concat vs json_escape
        bench("naive_escape_1k", 10_000, || {
            let s = "x".repeat(1024);
            let mut out = String::new();
            for c in s.chars() {
                if c == '"' {
                    out.push_str("\\\"");
                } else {
                    out.push(c);
                }
            }
            let _ = out;
        });
    }
}
