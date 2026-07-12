//! Native nsupa standard library — Supabase client (PostgREST + Auth + Storage).
//!
//! Import with `import "nsupa"` (or `import "std/nsupa"`).
//!
//! ## Quick start
//!
//! ```niao
//! import "nsupa"
//!
//! let client = nsupa.connect("https://xyz.supabase.co", "anon-key")
//!
//! // SELECT with filter
//! let q = nsupa.from(client, "users")
//! nsupa.eq(q, "active", "true")
//! nsupa.limit(q, 10)
//! let rows = nsupa.select(q)
//!
//! // INSERT
//! let q2 = nsupa.from(client, "posts")
//! let row = nsupa.insert(q2, {"title": "hello", "body": "world"})
//!
//! // Auth
//! let session = nsupa.auth_sign_in(client, "user@example.com", "password")
//!
//! nsupa.close(client)
//! ```

mod auth;
mod client;
mod common;
mod query;
mod storage;

use crate::{
    error_value, json_stringify, NativeFn, NiaoResult, RuntimeError, Value, ValueRef,
};
use client::SupaClient;
use common::{http_delete_json, http_get_json, http_patch_json, http_post_json};
use niao_ast::Span;
use niao_errors::codes;
use query::QueryHandle;
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn supa_arity_err(span: Span, name: &str, expected: &str, got: usize) -> RuntimeError {
    RuntimeError::at(
        span,
        codes::E2820_NSUPA_ARITY,
        format!("{name}() expects {expected} argument(s), got {got}"),
    )
}

fn supa_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2821_NSUPA_ERROR, "nsupa_error", msg.into(), span)
}

fn supa_type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2822_NSUPA_TYPE, msg.into())
}

fn supa_auth_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2823_NSUPA_AUTH, "nsupa_auth_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Arity guards
// ---------------------------------------------------------------------------

#[inline]
fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(supa_arity_err(span, name, &n.to_string(), args.len()));
    }
    Ok(())
}

#[inline]
fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(supa_arity_err(
            span,
            name,
            &format!("{min}..={max}"),
            args.len(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Argument extractors
// ---------------------------------------------------------------------------

#[inline]
fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(supa_type_err(
            span,
            format!("{name}: arg {idx} must be a string, got {}", other.type_name()),
        )),
    }
}

#[inline]
fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(supa_type_err(
            span,
            format!("{name}: arg {idx} must be an int, got {}", other.type_name()),
        )),
    }
}

#[inline]
fn obj_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<ValueRef> {
    match &*args[idx].borrow() {
        Value::Object(_) => Ok(args[idx].clone()),
        other => Err(supa_type_err(
            span,
            format!("{name}: arg {idx} must be an object, got {}", other.type_name()),
        )),
    }
}

/// Serialise a Niao value to a JSON string (reuses `json_stringify` builtin).
fn to_json(v: &ValueRef, name: &str, span: Span) -> NiaoResult<String> {
    let result = json_stringify(&[v.clone()], span)?;
    // Clone immediately to release the borrow before the function returns.
    let s = match &*result.borrow() {
        Value::String(s) => Ok(s.clone()),
        _ => Err(supa_type_err(
            span,
            format!("{name}: failed to serialise value to JSON"),
        )),
    };
    s
}

// ---------------------------------------------------------------------------
// nsupa_connect / nsupa_close
// ---------------------------------------------------------------------------

fn nsupa_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsupa_connect", span)?;
    let url = string_arg(args, 0, "nsupa_connect", span)?;
    let anon_key = string_arg(args, 1, "nsupa_connect", span)?;
    let service_key = if args.len() == 3 {
        Some(string_arg(args, 2, "nsupa_connect", span)?)
    } else {
        None
    };
    let id = client::register(SupaClient {
        url,
        anon_key,
        service_key,
        auth_token: None,
    });
    Ok(Value::Int(id).ref_cell())
}

fn nsupa_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsupa_close", span)?;
    let id = int_arg(args, 0, "nsupa_close", span)?;
    client::remove(id);
    Ok(Value::Bool(true).ref_cell())
}

// ---------------------------------------------------------------------------
// nsupa_from — allocate a query handle for a table
// ---------------------------------------------------------------------------

fn nsupa_from(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsupa_from", span)?;
    let client_id = int_arg(args, 0, "nsupa_from", span)?;
    let table = string_arg(args, 1, "nsupa_from", span)?;

    if client::with_client(client_id, "nsupa_from", span, |_| Ok(())).is_err() {
        return Ok(supa_err(
            span,
            format!("nsupa_from: invalid client handle {client_id}"),
        ));
    }
    let qid = query::register(QueryHandle::new(client_id, table));
    Ok(Value::Int(qid).ref_cell())
}

// ---------------------------------------------------------------------------
// Filter helpers — mutate and return the same query handle id
// ---------------------------------------------------------------------------

macro_rules! filter_fn {
    ($fn_name:ident, $builtin:literal, $op:literal) => {
        fn $fn_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
            arity(args, 3, $builtin, span)?;
            let qid = int_arg(args, 0, $builtin, span)?;
            let col = string_arg(args, 1, $builtin, span)?;
            let val = value_to_filter_str(&args[2]);
            let filter_str = format!("{col}={}.{val}", $op);
            match query::with_query_mut(qid, $builtin, |qh| {
                qh.filters.push(filter_str.clone());
                Ok(())
            }) {
                Ok(id) => Ok(Value::Int(id).ref_cell()),
                Err(e) => Ok(supa_err(span, e)),
            }
        }
    };
}

filter_fn!(nsupa_eq,  "nsupa_eq",  "eq");
filter_fn!(nsupa_neq, "nsupa_neq", "neq");
filter_fn!(nsupa_gt,  "nsupa_gt",  "gt");
filter_fn!(nsupa_lt,  "nsupa_lt",  "lt");
filter_fn!(nsupa_gte, "nsupa_gte", "gte");
filter_fn!(nsupa_lte, "nsupa_lte", "lte");

fn nsupa_order(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsupa_order", span)?;
    let qid = int_arg(args, 0, "nsupa_order", span)?;
    let col = string_arg(args, 1, "nsupa_order", span)?;
    let dir = if args.len() == 3 {
        string_arg(args, 2, "nsupa_order", span)?.to_lowercase()
    } else {
        "asc".to_string()
    };
    let order_str = format!("{col}.{dir}");
    match query::with_query_mut(qid, "nsupa_order", |qh| {
        qh.order = Some(order_str.clone());
        Ok(())
    }) {
        Ok(id) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(supa_err(span, e)),
    }
}

fn nsupa_limit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsupa_limit", span)?;
    let qid = int_arg(args, 0, "nsupa_limit", span)?;
    let n = int_arg(args, 1, "nsupa_limit", span)?;
    match query::with_query_mut(qid, "nsupa_limit", |qh| {
        qh.limit = Some(n.max(0) as usize);
        Ok(())
    }) {
        Ok(id) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(supa_err(span, e)),
    }
}

fn nsupa_offset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsupa_offset", span)?;
    let qid = int_arg(args, 0, "nsupa_offset", span)?;
    let n = int_arg(args, 1, "nsupa_offset", span)?;
    match query::with_query_mut(qid, "nsupa_offset", |qh| {
        qh.offset = Some(n.max(0) as usize);
        Ok(())
    }) {
        Ok(id) => Ok(Value::Int(id).ref_cell()),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Terminal: SELECT  (consumes query handle, fires GET → PostgREST)
// ---------------------------------------------------------------------------

fn nsupa_select(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsupa_select", span)?;
    let qid = int_arg(args, 0, "nsupa_select", span)?;

    // Optional column list — set before we consume the handle.
    if args.len() == 2 {
        let cols = string_arg(args, 1, "nsupa_select", span)?;
        if let Err(e) = query::with_query_mut(qid, "nsupa_select", |qh| {
            qh.select_cols = cols.clone();
            Ok(())
        }) {
            return Ok(supa_err(span, e));
        }
    }

    let qh = match query::take(qid, "nsupa_select") {
        Ok(q) => q,
        Err(e) => return Ok(supa_err(span, e)),
    };

    let result = client::with_client(qh.client_id, "nsupa_select", span, |cl| {
        let url = qh.rest_url(&cl.url);
        http_get_json(&url, cl.api_key(), cl.auth_token.as_deref()).map_err(|e| e.0)
    });

    match result {
        Ok(v) => Ok(v),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Terminal: INSERT  (consumes query handle, fires POST → PostgREST)
// ---------------------------------------------------------------------------

fn nsupa_insert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsupa_insert", span)?;
    let qid = int_arg(args, 0, "nsupa_insert", span)?;
    let row = obj_arg(args, 1, "nsupa_insert", span)?;

    let body = to_json(&row, "nsupa_insert", span)?;

    let qh = match query::take(qid, "nsupa_insert") {
        Ok(q) => q,
        Err(e) => return Ok(supa_err(span, e)),
    };

    let result = client::with_client(qh.client_id, "nsupa_insert", span, |cl| {
        let url = format!("{}/rest/v1/{}", cl.url.trim_end_matches('/'), qh.table);
        http_post_json(&url, cl.api_key(), cl.auth_token.as_deref(), &body).map_err(|e| e.0)
    });

    match result {
        Ok(v) => Ok(v),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Terminal: UPDATE  (consumes query handle, fires PATCH → PostgREST)
// ---------------------------------------------------------------------------

fn nsupa_update(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsupa_update", span)?;
    let qid = int_arg(args, 0, "nsupa_update", span)?;
    let data = obj_arg(args, 1, "nsupa_update", span)?;

    let body = to_json(&data, "nsupa_update", span)?;

    let qh = match query::take(qid, "nsupa_update") {
        Ok(q) => q,
        Err(e) => return Ok(supa_err(span, e)),
    };

    let result = client::with_client(qh.client_id, "nsupa_update", span, |cl| {
        let url = qh.rest_url(&cl.url);
        http_patch_json(
            &url,
            cl.api_key(),
            cl.auth_token.as_deref(),
            &body,
            Some("return=representation"),
        )
        .map_err(|e| e.0)
    });

    match result {
        Ok(v) => Ok(v),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Terminal: DELETE  (consumes query handle, fires DELETE → PostgREST)
// ---------------------------------------------------------------------------

fn nsupa_delete(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsupa_delete", span)?;
    let qid = int_arg(args, 0, "nsupa_delete", span)?;

    let qh = match query::take(qid, "nsupa_delete") {
        Ok(q) => q,
        Err(e) => return Ok(supa_err(span, e)),
    };

    let result = client::with_client(qh.client_id, "nsupa_delete", span, |cl| {
        let url = qh.rest_url(&cl.url);
        http_delete_json(&url, cl.api_key(), cl.auth_token.as_deref()).map_err(|e| e.0)
    });

    match result {
        Ok(v) => Ok(v),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Auth — GoTrue REST
// ---------------------------------------------------------------------------

fn nsupa_auth_sign_up(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nsupa_auth_sign_up", span)?;
    let cid = int_arg(args, 0, "nsupa_auth_sign_up", span)?;
    let email = string_arg(args, 1, "nsupa_auth_sign_up", span)?;
    let password = string_arg(args, 2, "nsupa_auth_sign_up", span)?;

    let result = client::with_client(cid, "nsupa_auth_sign_up", span, |cl| {
        auth::sign_up(&cl.url, &cl.anon_key, &email, &password).map_err(|e| e.0)
    });

    match result {
        Ok(session) => {
            if let Some(tok) = auth::extract_access_token(&session) {
                let _ = client::with_client_mut(cid, "nsupa_auth_sign_up", span, |cl| {
                    cl.auth_token = Some(tok);
                    Ok(())
                });
            }
            Ok(session)
        }
        Err(e) => Ok(supa_auth_err(span, e)),
    }
}

fn nsupa_auth_sign_in(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nsupa_auth_sign_in", span)?;
    let cid = int_arg(args, 0, "nsupa_auth_sign_in", span)?;
    let email = string_arg(args, 1, "nsupa_auth_sign_in", span)?;
    let password = string_arg(args, 2, "nsupa_auth_sign_in", span)?;

    let result = client::with_client(cid, "nsupa_auth_sign_in", span, |cl| {
        auth::sign_in(&cl.url, &cl.anon_key, &email, &password).map_err(|e| e.0)
    });

    match result {
        Ok(session) => {
            if let Some(tok) = auth::extract_access_token(&session) {
                let _ = client::with_client_mut(cid, "nsupa_auth_sign_in", span, |cl| {
                    cl.auth_token = Some(tok);
                    Ok(())
                });
            }
            Ok(session)
        }
        Err(e) => Ok(supa_auth_err(span, e)),
    }
}

fn nsupa_auth_sign_out(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsupa_auth_sign_out", span)?;
    let cid = int_arg(args, 0, "nsupa_auth_sign_out", span)?;
    match client::with_client_mut(cid, "nsupa_auth_sign_out", span, |cl| {
        cl.auth_token = None;
        Ok(())
    }) {
        Ok(_) => Ok(Value::Bool(true).ref_cell()),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Storage — Supabase Storage REST
// ---------------------------------------------------------------------------

fn nsupa_storage_upload(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 4, "nsupa_storage_upload", span)?;
    let cid = int_arg(args, 0, "nsupa_storage_upload", span)?;
    let bucket = string_arg(args, 1, "nsupa_storage_upload", span)?;
    let path = string_arg(args, 2, "nsupa_storage_upload", span)?;
    let body = string_arg(args, 3, "nsupa_storage_upload", span)?;

    let result = client::with_client(cid, "nsupa_storage_upload", span, |cl| {
        storage::upload(&cl.url, &cl.bearer(), cl.api_key(), &bucket, &path, &body)
            .map_err(|e| e.0)
    });

    match result {
        Ok(v) => Ok(v),
        Err(e) => Ok(supa_err(span, e)),
    }
}

fn nsupa_storage_download(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nsupa_storage_download", span)?;
    let cid = int_arg(args, 0, "nsupa_storage_download", span)?;
    let bucket = string_arg(args, 1, "nsupa_storage_download", span)?;
    let path = string_arg(args, 2, "nsupa_storage_download", span)?;

    let result = client::with_client(cid, "nsupa_storage_download", span, |cl| {
        storage::download(&cl.url, &cl.bearer(), cl.api_key(), &bucket, &path)
            .map_err(|e| e.0)
    });

    match result {
        Ok(v) => Ok(v),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// RPC — PostgREST /rpc/<fn_name>
// ---------------------------------------------------------------------------

fn nsupa_rpc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nsupa_rpc", span)?;
    let cid = int_arg(args, 0, "nsupa_rpc", span)?;
    let fn_name = string_arg(args, 1, "nsupa_rpc", span)?;

    let body = if args.len() == 3 {
        to_json(&args[2], "nsupa_rpc", span)?
    } else {
        "{}".to_string()
    };

    let result = client::with_client(cid, "nsupa_rpc", span, |cl| {
        let url = format!("{}/rest/v1/rpc/{fn_name}", cl.url.trim_end_matches('/'));
        http_post_json(&url, cl.api_key(), cl.auth_token.as_deref(), &body).map_err(|e| e.0)
    });

    match result {
        Ok(v) => Ok(v),
        Err(e) => Ok(supa_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Drop a query handle without executing it
// ---------------------------------------------------------------------------

fn nsupa_drop_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsupa_drop_query", span)?;
    let qid = int_arg(args, 0, "nsupa_drop_query", span)?;
    query::remove(qid);
    Ok(Value::Bool(true).ref_cell())
}

// ---------------------------------------------------------------------------
// Utility: convert a Niao value to a PostgREST filter string
// ---------------------------------------------------------------------------

fn value_to_filter_str(v: &ValueRef) -> String {
    match &*v.borrow() {
        Value::String(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Nil => "null".to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Builtin registration table + namespace object
// ---------------------------------------------------------------------------

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("nsupa_connect",          Rc::new(nsupa_connect)),
        ("nsupa_close",            Rc::new(nsupa_close)),
        ("nsupa_from",             Rc::new(nsupa_from)),
        ("nsupa_select",           Rc::new(nsupa_select)),
        ("nsupa_insert",           Rc::new(nsupa_insert)),
        ("nsupa_update",           Rc::new(nsupa_update)),
        ("nsupa_delete",           Rc::new(nsupa_delete)),
        ("nsupa_eq",               Rc::new(nsupa_eq)),
        ("nsupa_neq",              Rc::new(nsupa_neq)),
        ("nsupa_gt",               Rc::new(nsupa_gt)),
        ("nsupa_lt",               Rc::new(nsupa_lt)),
        ("nsupa_gte",              Rc::new(nsupa_gte)),
        ("nsupa_lte",              Rc::new(nsupa_lte)),
        ("nsupa_order",            Rc::new(nsupa_order)),
        ("nsupa_limit",            Rc::new(nsupa_limit)),
        ("nsupa_offset",           Rc::new(nsupa_offset)),
        ("nsupa_drop_query",       Rc::new(nsupa_drop_query)),
        ("nsupa_auth_sign_up",     Rc::new(nsupa_auth_sign_up)),
        ("nsupa_auth_sign_in",     Rc::new(nsupa_auth_sign_in)),
        ("nsupa_auth_sign_out",    Rc::new(nsupa_auth_sign_out)),
        ("nsupa_storage_upload",   Rc::new(nsupa_storage_upload)),
        ("nsupa_storage_download", Rc::new(nsupa_storage_download)),
        ("nsupa_rpc",              Rc::new(nsupa_rpc)),
    ]
}

pub const MODULE_NAME: &str = "nsupa";
pub const MODULE_PATHS: &[&str] = &["nsupa", "std/nsupa"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "connect",          Rc::new(nsupa_connect));
    bind(&mut map, "close",            Rc::new(nsupa_close));
    bind(&mut map, "from",             Rc::new(nsupa_from));
    bind(&mut map, "select",           Rc::new(nsupa_select));
    bind(&mut map, "insert",           Rc::new(nsupa_insert));
    bind(&mut map, "update",           Rc::new(nsupa_update));
    bind(&mut map, "delete",           Rc::new(nsupa_delete));
    bind(&mut map, "eq",               Rc::new(nsupa_eq));
    bind(&mut map, "neq",              Rc::new(nsupa_neq));
    bind(&mut map, "gt",               Rc::new(nsupa_gt));
    bind(&mut map, "lt",               Rc::new(nsupa_lt));
    bind(&mut map, "gte",              Rc::new(nsupa_gte));
    bind(&mut map, "lte",              Rc::new(nsupa_lte));
    bind(&mut map, "order",            Rc::new(nsupa_order));
    bind(&mut map, "limit",            Rc::new(nsupa_limit));
    bind(&mut map, "offset",           Rc::new(nsupa_offset));
    bind(&mut map, "drop_query",       Rc::new(nsupa_drop_query));
    bind(&mut map, "auth_sign_up",     Rc::new(nsupa_auth_sign_up));
    bind(&mut map, "auth_sign_in",     Rc::new(nsupa_auth_sign_in));
    bind(&mut map, "auth_sign_out",    Rc::new(nsupa_auth_sign_out));
    bind(&mut map, "storage_upload",   Rc::new(nsupa_storage_upload));
    bind(&mut map, "storage_download", Rc::new(nsupa_storage_download));
    bind(&mut map, "rpc",              Rc::new(nsupa_rpc));
    Value::Object(map)
}
