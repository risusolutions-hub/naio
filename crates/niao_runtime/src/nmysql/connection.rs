//! Connection open/close, configure, and transaction builtins.

use super::config::{connect_opts_raw, connect_url, parse_connect_opts};
use super::handles::{self, alloc_conn, remove_conn};
use crate::{error_from_runtime, error_value, NiaoResult, Value, ValueRef};
use mysql::prelude::*;
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

use super::common::*;

fn nmysql_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E1911_NMYSQL_ERROR, "nmysql_error", msg.into(), span)
}

fn ok_nil() -> ValueRef {
    Value::Nil.ref_cell()
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

fn ok_string(s: impl Into<String>) -> ValueRef {
    Value::String(s.into()).ref_cell()
}

/// nmysql.connect(url) — connect via mysql:// URL.
// >>> import "nmysql"
// >>> nmysql.version()
pub fn nmysql_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_connect", span)?;
    let url = string_arg(args, 0, "nmysql_connect", span)?;
    match connect_url(&url) {
        Ok((client, _opts, reconnect)) => {
            let display = handles::redact_conninfo(&reconnect);
            Ok(ok_int(alloc_conn(client, reconnect, display) as i64))
        }
        Err(msg) => Ok(error_value(
            codes::E1917_NMYSQL_TLS,
            "nmysql_error",
            msg,
            span,
        )),
    }
}

/// nmysql.connect_opts(opts) — connect from host/user/password/database object.
// >>> import "nmysql"
// >>> nmysql.escape_literal("x")
// => "'x'"
pub fn nmysql_connect_opts(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_connect_opts", span)?;
    let (opts, reconnect, display) = parse_connect_opts(&args[0], span)?;
    match connect_opts_raw(opts) {
        Ok(client) => Ok(ok_int(alloc_conn(client, reconnect, display) as i64)),
        Err(msg) => Ok(error_value(
            codes::E1917_NMYSQL_TLS,
            "nmysql_error",
            msg,
            span,
        )),
    }
}

/// nmysql.close(conn) — close connection and invalidate related statements.
// >>> import "nmysql"
// >>> nmysql.quote_ident("t")
// => "`t`"
pub fn nmysql_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_close", span)?;
    let id = conn_arg(args, 0, "nmysql_close", span)?;
    if remove_conn(id).is_some() {
        Ok(ok_nil())
    } else {
        Ok(nmysql_error(
            span,
            format!("nmysql_close(): invalid or closed connection handle {id}"),
        ))
    }
}

/// nmysql.ping(conn) — SELECT 1 health check.
// >>> import "nmysql"
// >>> nmysql.version()
pub fn nmysql_ping(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_ping", span)?;
    let id = conn_arg(args, 0, "nmysql_ping", span)?;
    handles::with_conn_mut(id, "nmysql_ping", span, |handle| {
        handle
            .client_mut()
            .query_drop("SELECT 1")
            .map_err(|e| e.to_string())
    })
    .map(|_| ok_bool(true))
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.configure(conn, opts) — session settings: max_execution_time, time_zone, wait_timeout, charset.
// >>> import "nmysql"
// >>> nmysql.quote_ident("db")
// => "`db`"
pub fn nmysql_configure(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmysql_configure", span)?;
    let id = conn_arg(args, 0, "nmysql_configure", span)?;
    let opts = object_arg(args, 1, "nmysql_configure", span)?;
    handles::with_conn_mut(id, "nmysql_configure", span, |handle| {
        for (key, val) in opts {
            let val_ref = &*val.borrow();
            match key.as_str() {
                "max_execution_time" => match val_ref {
                    Value::Int(ms) if *ms >= 0 => {
                        handle
                            .client_mut()
                            .query_drop(format!("SET SESSION max_execution_time = {ms}"))
                            .map_err(|e| e.to_string())?;
                    }
                    other => {
                        return Err(format!(
                            "max_execution_time expects non-negative int (ms), got {}",
                            other.type_name()
                        ));
                    }
                },
                "wait_timeout" => match val_ref {
                    Value::Int(secs) if *secs >= 0 => {
                        handle
                            .client_mut()
                            .query_drop(format!("SET SESSION wait_timeout = {secs}"))
                            .map_err(|e| e.to_string())?;
                    }
                    other => {
                        return Err(format!(
                            "wait_timeout expects non-negative int (seconds), got {}",
                            other.type_name()
                        ));
                    }
                },
                "time_zone" => match val_ref {
                    Value::String(s) => {
                        let lit = super::types::quote_literal(s);
                        handle
                            .client_mut()
                            .query_drop(format!("SET time_zone = {lit}"))
                            .map_err(|e| e.to_string())?;
                    }
                    other => {
                        return Err(format!("time_zone expects string, got {}", other.type_name()));
                    }
                },
                "charset" => match val_ref {
                    Value::String(s) => {
                        // charset names are restricted identifiers
                        if !s
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                        {
                            return Err("charset contains invalid characters".into());
                        }
                        handle
                            .client_mut()
                            .query_drop(format!("SET NAMES {s}"))
                            .map_err(|e| e.to_string())?;
                    }
                    other => {
                        return Err(format!("charset expects string, got {}", other.type_name()));
                    }
                },
                other => return Err(format!("unknown configure option \"{other}\"")),
            }
        }
        Ok(())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.conninfo(conn) — redacted connection info.
// >>> import "nmysql"
// >>> nmysql.escape_literal("")
// => "''"
pub fn nmysql_conninfo(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_conninfo", span)?;
    let id = conn_arg(args, 0, "nmysql_conninfo", span)?;
    match handles::conn_display(id) {
        Some(p) => Ok(ok_string(p)),
        None => Ok(nmysql_error(
            span,
            format!("nmysql_conninfo(): invalid connection handle {id}"),
        )),
    }
}

/// nmysql.server_version(conn) — SELECT VERSION().
// >>> import "nmysql"
// >>> nmysql.version()
pub fn nmysql_server_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_server_version", span)?;
    let id = conn_arg(args, 0, "nmysql_server_version", span)?;
    handles::with_conn_mut(id, "nmysql_server_version", span, |handle| {
        handle
            .client_mut()
            .query_first::<String, _>("SELECT VERSION()")
            .map_err(|e| e.to_string())
            .and_then(|opt| {
                opt.ok_or_else(|| "VERSION() returned no rows".to_string())
            })
    })
    .map(ok_string)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.is_in_transaction(conn) — client-tracked transaction flag.
// >>> import "nmysql"
// >>> nmysql.quote_ident("x")
// => "`x`"
pub fn nmysql_is_in_transaction(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_is_in_transaction", span)?;
    let id = conn_arg(args, 0, "nmysql_is_in_transaction", span)?;
    handles::with_conn_mut(id, "nmysql_is_in_transaction", span, |handle| {
        Ok(handle.in_transaction)
    })
    .map(ok_bool)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.last_insert_id(conn) — last AUTO_INCREMENT id on this connection.
// >>> import "nmysql"
// >>> nmysql.escape_literal("1")
// => "'1'"
pub fn nmysql_last_insert_id(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_last_insert_id", span)?;
    let id = conn_arg(args, 0, "nmysql_last_insert_id", span)?;
    handles::with_conn_mut(id, "nmysql_last_insert_id", span, |handle| {
        Ok(handle.last_insert_id as i64)
    })
    .map(ok_int)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.affected_rows(conn) — rows affected by last DML.
// >>> import "nmysql"
// >>> nmysql.quote_ident("id")
// => "`id`"
pub fn nmysql_affected_rows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_affected_rows", span)?;
    let id = conn_arg(args, 0, "nmysql_affected_rows", span)?;
    handles::with_conn_mut(id, "nmysql_affected_rows", span, |handle| {
        Ok(handle.affected_rows as i64)
    })
    .map(ok_int)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.begin(conn, opts?) — START TRANSACTION with optional isolation/read_only.
// >>> import "nmysql"
// >>> nmysql.version()
pub fn nmysql_begin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmysql_begin", span)?;
    let id = conn_arg(args, 0, "nmysql_begin", span)?;
    let opts = if args.len() == 2 {
        object_arg(args, 1, "nmysql_begin", span)?
    } else {
        HashMap::new()
    };
    handles::with_conn_mut(id, "nmysql_begin", span, |handle| {
        if let Some(iso_ref) = opts.get("isolation") {
            let iso = match &*iso_ref.borrow() {
                Value::String(s) => s.clone(),
                other => {
                    return Err(format!(
                        "isolation expects string, got {}",
                        other.type_name()
                    ));
                }
            };
            let level = match iso.to_lowercase().as_str() {
                "read uncommitted" | "read_uncommitted" => "READ UNCOMMITTED",
                "read committed" | "read_committed" => "READ COMMITTED",
                "repeatable read" | "repeatable_read" => "REPEATABLE READ",
                "serializable" => "SERIALIZABLE",
                other => return Err(format!("unknown isolation level \"{other}\"")),
            };
            handle
                .client_mut()
                .query_drop(format!("SET TRANSACTION ISOLATION LEVEL {level}"))
                .map_err(|e| e.to_string())?;
        }
        if let Some(ro_ref) = opts.get("read_only") {
            if bool_from_value(&*ro_ref.borrow())? {
                handle
                    .client_mut()
                    .query_drop("SET TRANSACTION READ ONLY")
                    .map_err(|e| e.to_string())?;
            }
        }
        handle
            .client_mut()
            .query_drop("START TRANSACTION")
            .map_err(|e| e.to_string())?;
        handle.in_transaction = true;
        Ok(())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.commit(conn) — COMMIT.
// >>> import "nmysql"
// >>> nmysql.escape_literal("ok")
// => "'ok'"
pub fn nmysql_commit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_commit", span)?;
    let id = conn_arg(args, 0, "nmysql_commit", span)?;
    handles::with_conn_mut(id, "nmysql_commit", span, |handle| {
        handle
            .client_mut()
            .query_drop("COMMIT")
            .map_err(|e| e.to_string())?;
        handle.in_transaction = false;
        Ok(())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.rollback(conn) — ROLLBACK.
// >>> import "nmysql"
// >>> nmysql.quote_ident("t")
// => "`t`"
pub fn nmysql_rollback(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_rollback", span)?;
    let id = conn_arg(args, 0, "nmysql_rollback", span)?;
    handles::with_conn_mut(id, "nmysql_rollback", span, |handle| {
        handle
            .client_mut()
            .query_drop("ROLLBACK")
            .map_err(|e| e.to_string())?;
        handle.in_transaction = false;
        Ok(())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.savepoint(conn, name) — SAVEPOINT.
// >>> import "nmysql"
// >>> nmysql.quote_ident("sp1")
// => "`sp1`"
pub fn nmysql_savepoint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmysql_savepoint", span)?;
    let id = conn_arg(args, 0, "nmysql_savepoint", span)?;
    let name = string_arg(args, 1, "nmysql_savepoint", span)?;
    let ident = super::types::quote_ident(&name);
    handles::with_conn_mut(id, "nmysql_savepoint", span, |handle| {
        handle
            .client_mut()
            .query_drop(format!("SAVEPOINT {ident}"))
            .map_err(|e| e.to_string())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.rollback_to(conn, name) — ROLLBACK TO SAVEPOINT.
// >>> import "nmysql"
// >>> nmysql.quote_ident("sp1")
// => "`sp1`"
pub fn nmysql_rollback_to(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmysql_rollback_to", span)?;
    let id = conn_arg(args, 0, "nmysql_rollback_to", span)?;
    let name = string_arg(args, 1, "nmysql_rollback_to", span)?;
    let ident = super::types::quote_ident(&name);
    handles::with_conn_mut(id, "nmysql_rollback_to", span, |handle| {
        handle
            .client_mut()
            .query_drop(format!("ROLLBACK TO SAVEPOINT {ident}"))
            .map_err(|e| e.to_string())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.version() — nmysql library version string.
// >>> import "nmysql"
// >>> type(nmysql.version())
// => "string"
pub fn nmysql_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nmysql_version", span)?;
    Ok(ok_string(env!("CARGO_PKG_VERSION")))
}

/// nmysql.escape_literal(s) — SQL string literal with quotes.
// >>> import "nmysql"
// >>> nmysql.escape_literal("a'b")
// => "'a''b'"
pub fn nmysql_escape_literal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_escape_literal", span)?;
    let s = string_arg(args, 0, "nmysql_escape_literal", span)?;
    Ok(ok_string(super::types::quote_literal(&s)))
}

/// nmysql.quote_ident(s) — backtick-quoted identifier.
// >>> import "nmysql"
// >>> nmysql.quote_ident("a`b")
// => "`a``b`"
pub fn nmysql_quote_ident(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_quote_ident", span)?;
    let s = string_arg(args, 0, "nmysql_quote_ident", span)?;
    Ok(ok_string(super::types::quote_ident(&s)))
}
