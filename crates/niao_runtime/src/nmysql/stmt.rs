//! Prepared statement builtins.

use super::handles::{self, alloc_stmt, remove_stmt};
use super::query::{collect_rows, parse_row_format, RowFormat};
use super::types::{bound_to_mysql, niao_to_bound, rewrite_placeholders};
use crate::{error_from_runtime, error_value, NiaoResult, RuntimeError, Value, ValueRef};
use mysql::prelude::*;
use mysql::Row;
use niao_ast::Span;
use niao_errors::codes;

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

fn stmt_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<u64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id as u64),
        other => Err(RuntimeError::at(
            span,
            codes::E1912_NMYSQL_INVALID_HANDLE,
            format!(
                "{name}() expects statement handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

/// nmysql.prepare(conn, sql) — prepare statement (validates SQL).
// >>> import "nmysql"
// >>> nmysql.quote_ident("stmt")
// => "`stmt`"
pub fn nmysql_prepare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmysql_prepare", span)?;
    let conn_id = conn_arg(args, 0, "nmysql_prepare", span)?;
    let sql = string_arg(args, 1, "nmysql_prepare", span)?;
    let sql = rewrite_placeholders(&sql);
    let result = handles::with_conn_mut(conn_id, "nmysql_prepare", span, |handle| {
        // Validate by preparing on the server; discard server handle (client-side bind model).
        let _stmt = handle.client_mut().prep(sql.as_str()).map_err(|e| e.to_string())?;
        Ok(())
    });
    match result {
        Ok(()) => Ok(ok_int(alloc_stmt(conn_id, sql) as i64)),
        Err(e) => Ok(error_from_runtime(&e)),
    }
}

/// nmysql.bind(stmt, index, value) — positional bind (1-based).
// >>> import "nmysql"
// >>> nmysql.escape_literal("v")
// => "'v'"
pub fn nmysql_bind(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmysql_bind", span)?;
    let stmt_id = stmt_arg(args, 0, "nmysql_bind", span)?;
    let index = int_arg(args, 1, "nmysql_bind", span)?;
    if index < 1 {
        return Ok(nmysql_error(
            span,
            "nmysql_bind(): bind index must be >= 1",
        ));
    }
    let bound = niao_to_bound(&*args[2].borrow(), span)?;
    handles::with_stmt_mut(stmt_id, "nmysql_bind", span, |stmt| {
        stmt.params.retain(|(i, _)| *i != index as i32);
        stmt.params.push((index as i32, bound));
        Ok(())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.stmt_exec(stmt) — execute prepared statement without rows.
// >>> import "nmysql"
// >>> nmysql.version()
pub fn nmysql_stmt_exec(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_stmt_exec", span)?;
    let stmt_id = stmt_arg(args, 0, "nmysql_stmt_exec", span)?;
    handles::with_stmt_and_conn(stmt_id, "nmysql_stmt_exec", span, |stmt, conn| {
        let sql = stmt.sql.clone();
        let mut sorted = stmt.params.clone();
        sorted.sort_by_key(|(i, _)| *i);
        let params: Vec<_> = sorted.into_iter().map(|(_, v)| v).collect();
        let mysql_params = bound_to_mysql(&params);
        conn.client_mut()
            .exec_drop(sql.as_str(), mysql_params)
            .map_err(|e| e.to_string())?;
        conn.refresh_meta();
        Ok(conn.affected_rows as i64)
    })
    .map(ok_int)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.stmt_query(stmt, format?) — execute prepared statement with rows.
// >>> import "nmysql"
// >>> nmysql.quote_ident("q")
// => "`q`"
pub fn nmysql_stmt_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmysql_stmt_query", span)?;
    let stmt_id = stmt_arg(args, 0, "nmysql_stmt_query", span)?;
    let format = if args.len() == 2 {
        parse_row_format(&string_arg(args, 1, "nmysql_stmt_query", span)?)
            .map_err(|msg| RuntimeError::at(span, codes::E1911_NMYSQL_ERROR, msg))?
    } else {
        RowFormat::Object
    };
    handles::with_stmt_and_conn(stmt_id, "nmysql_stmt_query", span, |stmt, conn| {
        let sql = stmt.sql.clone();
        let mut sorted = stmt.params.clone();
        sorted.sort_by_key(|(i, _)| *i);
        let params: Vec<_> = sorted.into_iter().map(|(_, v)| v).collect();
        let mysql_params = bound_to_mysql(&params);
        let rows: Vec<Row> = conn
            .client_mut()
            .exec(sql.as_str(), mysql_params)
            .map_err(|e| e.to_string())?;
        conn.refresh_meta();
        collect_rows(rows, format)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.stmt_reset(stmt) — clear bindings.
// >>> import "nmysql"
// >>> nmysql.escape_literal("r")
// => "'r'"
pub fn nmysql_stmt_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_stmt_reset", span)?;
    let stmt_id = stmt_arg(args, 0, "nmysql_stmt_reset", span)?;
    handles::with_stmt_mut(stmt_id, "nmysql_stmt_reset", span, |stmt| {
        stmt.params.clear();
        Ok(())
    })
    .map(|_| ok_nil())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.finalize(stmt) — free statement handle.
// >>> import "nmysql"
// >>> nmysql.quote_ident("f")
// => "`f`"
pub fn nmysql_finalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmysql_finalize", span)?;
    let stmt_id = stmt_arg(args, 0, "nmysql_finalize", span)?;
    if remove_stmt(stmt_id).is_some() {
        Ok(ok_nil())
    } else {
        Ok(nmysql_error(
            span,
            format!("nmysql_finalize(): invalid statement handle {stmt_id}"),
        ))
    }
}
