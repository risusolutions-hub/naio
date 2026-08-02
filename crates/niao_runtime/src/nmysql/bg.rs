//! Async query/execute via shared background task pool.

use super::common::*;
use super::config::connect_url;
use super::handles::{self, ConnHandle, ConnInner};
use super::query::{exec_on_conn, parse_row_format, query_on_conn, RowFormat};
use super::types::value_to_async;
use crate::async_tasks::{
    spawn_async, task_done, task_result_value, task_wait_loop, with_task, AsyncValue,
};
use crate::{error_value, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;

fn nmysql_async_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E1911_NMYSQL_ERROR, "nmysql_error", msg.into(), span)
}

fn capture_reconnect(conn_id: u64, span: Span) -> NiaoResult<String> {
    handles::conn_reconnect_url(conn_id).ok_or_else(|| {
        RuntimeError::at(
            span,
            codes::E1912_NMYSQL_INVALID_HANDLE,
            format!("invalid connection handle {conn_id}"),
        )
    })
}

/// nmysql.async_exec(conn, sql, params?) — background exec; returns task id.
// >>> import "nmysql"
// >>> nmysql.version()
pub fn nmysql_async_exec(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmysql_async_exec", span)?;
    let conn_id = conn_arg(args, 0, "nmysql_async_exec", span)?;
    let sql = string_arg(args, 1, "nmysql_async_exec", span)?;
    let params = if args.len() == 3 {
        params_array_arg(args, 2, "nmysql_async_exec", span)?
    } else {
        Vec::new()
    };
    let reconnect = capture_reconnect(conn_id, span)?;
    let id = spawn_async(move || {
        let (client, _, url) = connect_url(&reconnect).map_err(|e| e.to_string())?;
        let mut handle = ConnHandle {
            inner: ConnInner::Direct(client),
            reconnect_url: url,
            display: String::new(),
            in_transaction: false,
            last_insert_id: 0,
            affected_rows: 0,
        };
        let n = exec_on_conn(&mut handle, &sql, &params)?;
        Ok(AsyncValue::int(n as i64))
    });
    Ok(Value::Int(id as i64).ref_cell())
}

/// nmysql.async_query(conn, sql, params?, format?) — background query; returns task id.
// >>> import "nmysql"
// >>> nmysql.quote_ident("a")
// => "`a`"
pub fn nmysql_async_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nmysql_async_query", span)?;
    let conn_id = conn_arg(args, 0, "nmysql_async_query", span)?;
    let sql = string_arg(args, 1, "nmysql_async_query", span)?;
    let params = if args.len() >= 3 {
        params_array_arg(args, 2, "nmysql_async_query", span)?
    } else {
        Vec::new()
    };
    let format = if args.len() >= 4 {
        parse_row_format(&string_arg(args, 3, "nmysql_async_query", span)?)
            .map_err(|msg| RuntimeError::at(span, codes::E1911_NMYSQL_ERROR, msg))?
    } else {
        RowFormat::Object
    };
    let reconnect = capture_reconnect(conn_id, span)?;
    let id = spawn_async(move || {
        let (client, _, url) = connect_url(&reconnect).map_err(|e| e.to_string())?;
        let mut handle = ConnHandle {
            inner: ConnInner::Direct(client),
            reconnect_url: url,
            display: String::new(),
            in_transaction: false,
            last_insert_id: 0,
            affected_rows: 0,
        };
        let result = query_on_conn(&mut handle, &sql, &params, format)?;
        Ok(value_to_async(&result))
    });
    Ok(Value::Int(id as i64).ref_cell())
}

/// nmysql.task_done(task) — whether background task finished.
// >>> import "nmysql"
// >>> nmysql.escape_literal("t")
// => "'t'"
pub fn nmysql_task_done(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_task_done", span)?;
    let id = task_arg(args, 0, "nmysql_task_done", span)?;
    with_task(
        id,
        "nmysql_task_done",
        span,
        codes::E1915_NMYSQL_TASK_NOT_FOUND,
        "nmysql task cancelled",
        |s, m| nmysql_async_error(s, m),
        |state| Ok(Value::Bool(task_done(state)).ref_cell()),
    )
}

/// nmysql.task_wait(task) — block until task finishes.
// >>> import "nmysql"
// >>> nmysql.version()
pub fn nmysql_task_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_task_wait", span)?;
    let id = task_arg(args, 0, "nmysql_task_wait", span)?;
    task_wait_loop(id);
    with_task(
        id,
        "nmysql_task_wait",
        span,
        codes::E1915_NMYSQL_TASK_NOT_FOUND,
        "nmysql task cancelled",
        |s, m| nmysql_async_error(s, m),
        |_| Ok(Value::Nil.ref_cell()),
    )
}

/// nmysql.task_result(task) — result value of finished task.
// >>> import "nmysql"
// >>> nmysql.quote_ident("r")
// => "`r`"
pub fn nmysql_task_result(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_task_result", span)?;
    let id = task_arg(args, 0, "nmysql_task_result", span)?;
    with_task(
        id,
        "nmysql_task_result",
        span,
        codes::E1915_NMYSQL_TASK_NOT_FOUND,
        "nmysql task cancelled",
        |s, m| nmysql_async_error(s, m),
        |state| {
            Ok(task_result_value(state, span, "nmysql task cancelled", |s, m| {
                nmysql_async_error(s, m)
            }))
        },
    )
}

/// nmysql.task_cancel(task) — request cancel; returns bool.
// >>> import "nmysql"
// >>> nmysql.escape_literal("c")
// => "'c'"
pub fn nmysql_task_cancel(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_task_cancel", span)?;
    let id = task_arg(args, 0, "nmysql_task_cancel", span)?;
    let cancelled =
        crate::async_tasks::cancel_task(id, span, codes::E1915_NMYSQL_TASK_NOT_FOUND)?;
    Ok(Value::Bool(cancelled).ref_cell())
}

fn task_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id as u64),
        other => Err(RuntimeError::at(
            span,
            codes::E1910_NMYSQL_ARITY,
            format!(
                "{name}() expects task id as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}
