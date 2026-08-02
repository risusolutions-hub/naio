//! Native nmysql standard library — MySQL / MariaDB client.

mod bg;
mod common;
mod config;
mod connection;
pub(crate) mod handles;
mod pool;
pub(crate) mod query;
mod schema;
mod stmt;
pub(crate) mod types;

use crate::{error_from_runtime, error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use bg::{
    nmysql_async_exec, nmysql_async_query, nmysql_task_cancel, nmysql_task_done, nmysql_task_result,
    nmysql_task_wait,
};
use common::*;
use connection::{
    nmysql_affected_rows, nmysql_begin, nmysql_close, nmysql_commit, nmysql_configure,
    nmysql_connect, nmysql_connect_opts, nmysql_conninfo, nmysql_escape_literal,
    nmysql_is_in_transaction, nmysql_last_insert_id, nmysql_ping, nmysql_quote_ident,
    nmysql_rollback, nmysql_rollback_to, nmysql_savepoint, nmysql_server_version, nmysql_version,
};
use mysql::prelude::*;
use niao_ast::Span;
use niao_errors::codes;
use pool::{nmysql_pool, nmysql_pool_close, nmysql_pool_get, nmysql_pool_status};
use query::{
    batch_on_conn, exec_on_conn, insert_on_conn, query_column_on_conn, query_on_conn,
    query_row_on_conn, query_value_on_conn, RowFormat,
};
use schema::{list_indexes, list_tables, parse_migrations, run_migrations, table_exists, table_info};
use stmt::{
    nmysql_bind, nmysql_finalize, nmysql_prepare, nmysql_stmt_exec, nmysql_stmt_query,
    nmysql_stmt_reset,
};
use std::collections::HashMap;
use std::rc::Rc;

fn nmysql_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E1911_NMYSQL_ERROR, "nmysql_error", msg.into(), span)
}

fn ok_int(n: i64) -> ValueRef {
    Value::Int(n).ref_cell()
}

fn ok_bool(b: bool) -> ValueRef {
    Value::Bool(b).ref_cell()
}

/// nmysql.exec(conn, sql, params?) — DDL/DML; returns affected row count.
// >>> import "nmysql"
// >>> nmysql.version()
fn nmysql_exec(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmysql_exec", span)?;
    let id = conn_arg(args, 0, "nmysql_exec", span)?;
    let sql = string_arg(args, 1, "nmysql_exec", span)?;
    let params = if args.len() == 3 {
        params_array_arg(args, 2, "nmysql_exec", span)?
    } else {
        Vec::new()
    };
    handles::with_conn_mut(id, "nmysql_exec", span, |handle| {
        exec_on_conn(handle, &sql, &params)
    })
    .map(|n| ok_int(n as i64))
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.exec_many(conn, sql_list) — multiple statements in one transaction.
// >>> import "nmysql"
// >>> nmysql.quote_ident("t")
// => "`t`"
fn nmysql_exec_many(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmysql_exec_many", span)?;
    let id = conn_arg(args, 0, "nmysql_exec_many", span)?;
    let statements = sql_list_arg(args, 1, "nmysql_exec_many", span)?;
    handles::with_conn_mut(id, "nmysql_exec_many", span, |handle| {
        handle
            .client_mut()
            .query_drop("START TRANSACTION")
            .map_err(|e| e.to_string())?;
        let mut count = 0i64;
        let result = (|| {
            for sql in &statements {
                handle
                    .client_mut()
                    .query_drop(sql.as_str())
                    .map_err(|e| e.to_string())?;
                handle.refresh_meta();
                count += handle.affected_rows as i64;
            }
            handle
                .client_mut()
                .query_drop("COMMIT")
                .map_err(|e| e.to_string())?;
            Ok(count)
        })();
        if result.is_err() {
            let _ = handle.client_mut().query_drop("ROLLBACK");
        }
        result
    })
    .map(ok_int)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.migrate(conn, migrations) — apply {version, sql} migrations in order.
// >>> import "nmysql"
// >>> nmysql.escape_literal("m")
// => "'m'"
fn nmysql_migrate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmysql_migrate", span)?;
    let id = conn_arg(args, 0, "nmysql_migrate", span)?;
    let migrations = parse_migrations(&args[1], span)?;
    handles::with_conn_mut(id, "nmysql_migrate", span, |handle| {
        run_migrations(handle, &migrations).map_err(|e| e.to_string())
    })
    .map(ok_int)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.table_exists(conn, name) or table_exists(conn, schema, name).
// >>> import "nmysql"
// >>> nmysql.quote_ident("users")
// => "`users`"
fn nmysql_table_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmysql_table_exists", span)?;
    let id = conn_arg(args, 0, "nmysql_table_exists", span)?;
    let (schema, name) = if args.len() == 3 {
        (
            Some(string_arg(args, 1, "nmysql_table_exists", span)?),
            string_arg(args, 2, "nmysql_table_exists", span)?,
        )
    } else {
        (None, string_arg(args, 1, "nmysql_table_exists", span)?)
    };
    handles::with_conn_mut(id, "nmysql_table_exists", span, |handle| {
        table_exists(handle, schema.as_deref(), &name)
    })
    .map(ok_bool)
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.list_tables(conn, schema?) — table names in schema (default: current db).
// >>> import "nmysql"
// >>> nmysql.version()
fn nmysql_list_tables(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nmysql_list_tables", span)?;
    let id = conn_arg(args, 0, "nmysql_list_tables", span)?;
    let schema = if args.len() == 2 {
        Some(string_arg(args, 1, "nmysql_list_tables", span)?)
    } else {
        None
    };
    handles::with_conn_mut(id, "nmysql_list_tables", span, |handle| {
        list_tables(handle, schema.as_deref()).map(|names| {
            Value::Array(names.into_iter().map(|n| Value::String(n).ref_cell()).collect())
        })
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.table_info(conn, table, schema?) — column metadata.
// >>> import "nmysql"
// >>> nmysql.quote_ident("col")
// => "`col`"
fn nmysql_table_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmysql_table_info", span)?;
    let id = conn_arg(args, 0, "nmysql_table_info", span)?;
    let table = string_arg(args, 1, "nmysql_table_info", span)?;
    let schema = optional_string_arg(args, 2, "nmysql_table_info", span)?;
    handles::with_conn_mut(id, "nmysql_table_info", span, |handle| {
        table_info(handle, schema.as_deref(), &table).map(Value::Array)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.list_indexes(conn, schema?, table?) — index metadata.
// >>> import "nmysql"
// >>> nmysql.escape_literal("i")
// => "'i'"
fn nmysql_list_indexes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nmysql_list_indexes", span)?;
    let id = conn_arg(args, 0, "nmysql_list_indexes", span)?;
    let (schema, table) = match args.len() {
        1 => (None, None),
        2 => (Some(string_arg(args, 1, "nmysql_list_indexes", span)?), None),
        _ => (
            Some(string_arg(args, 1, "nmysql_list_indexes", span)?),
            Some(string_arg(args, 2, "nmysql_list_indexes", span)?),
        ),
    };
    handles::with_conn_mut(id, "nmysql_list_indexes", span, |handle| {
        list_indexes(handle, schema.as_deref(), table.as_deref()).map(Value::Array)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.query(conn, sql, params?, format?) — all rows as objects or array layout.
// >>> import "nmysql"
// >>> nmysql.version()
fn nmysql_query(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "nmysql_query", span)?;
    let id = conn_arg(args, 0, "nmysql_query", span)?;
    let sql = string_arg(args, 1, "nmysql_query", span)?;
    let (params, format) = if args.len() >= 3 {
        let params = params_array_arg(args, 2, "nmysql_query", span)?;
        let format = if args.len() == 4 {
            query::parse_row_format(&string_arg(args, 3, "nmysql_query", span)?)
                .map_err(|msg| RuntimeError::at(span, codes::E1911_NMYSQL_ERROR, msg))?
        } else {
            RowFormat::Object
        };
        (params, format)
    } else {
        (Vec::new(), RowFormat::Object)
    };
    handles::with_conn_mut(id, "nmysql_query", span, |handle| {
        query_on_conn(handle, &sql, &params, format)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.query_row(conn, sql, params?) — first row object or nil.
// >>> import "nmysql"
// >>> nmysql.quote_ident("r")
// => "`r`"
fn nmysql_query_row(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmysql_query_row", span)?;
    let id = conn_arg(args, 0, "nmysql_query_row", span)?;
    let sql = string_arg(args, 1, "nmysql_query_row", span)?;
    let params = if args.len() == 3 {
        params_array_arg(args, 2, "nmysql_query_row", span)?
    } else {
        Vec::new()
    };
    handles::with_conn_mut(id, "nmysql_query_row", span, |handle| {
        query_row_on_conn(handle, &sql, &params)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.query_value(conn, sql, params?) — first column of first row.
// >>> import "nmysql"
// >>> nmysql.escape_literal("1")
// => "'1'"
fn nmysql_query_value(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmysql_query_value", span)?;
    let id = conn_arg(args, 0, "nmysql_query_value", span)?;
    let sql = string_arg(args, 1, "nmysql_query_value", span)?;
    let params = if args.len() == 3 {
        params_array_arg(args, 2, "nmysql_query_value", span)?
    } else {
        Vec::new()
    };
    handles::with_conn_mut(id, "nmysql_query_value", span, |handle| {
        query_value_on_conn(handle, &sql, &params)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.query_column(conn, sql, params?) — first column of all rows.
// >>> import "nmysql"
// >>> nmysql.version()
fn nmysql_query_column(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmysql_query_column", span)?;
    let id = conn_arg(args, 0, "nmysql_query_column", span)?;
    let sql = string_arg(args, 1, "nmysql_query_column", span)?;
    let params = if args.len() == 3 {
        params_array_arg(args, 2, "nmysql_query_column", span)?
    } else {
        Vec::new()
    };
    handles::with_conn_mut(id, "nmysql_query_column", span, |handle| {
        query_column_on_conn(handle, &sql, &params)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.batch(conn, sql, rows) — repeated exec in one transaction.
// >>> import "nmysql"
// >>> nmysql.quote_ident("b")
// => "`b`"
fn nmysql_batch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmysql_batch", span)?;
    let id = conn_arg(args, 0, "nmysql_batch", span)?;
    let sql = string_arg(args, 1, "nmysql_batch", span)?;
    let rows_val = &*args[2].borrow();
    let rows = match rows_val {
        Value::Array(outer) => {
            let mut rows = Vec::with_capacity(outer.len());
            for row_ref in outer {
                match &*row_ref.borrow() {
                    Value::Array(cells) => {
                        let mut row = Vec::with_capacity(cells.len());
                        for cell in cells {
                            row.push(types::niao_to_bound(&*cell.borrow(), span)?);
                        }
                        rows.push(row);
                    }
                    other => {
                        return Ok(nmysql_error(
                            span,
                            format!(
                                "nmysql_batch() expects array of param arrays, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            rows
        }
        other => {
            return Ok(nmysql_error(
                span,
                format!(
                    "nmysql_batch() expects rows array as argument 3, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    handles::with_conn_mut(id, "nmysql_batch", span, |handle| {
        batch_on_conn(handle, &sql, &rows)
    })
    .map(|n| ok_int(n as i64))
    .or_else(|e| Ok(error_from_runtime(&e)))
}

/// nmysql.insert(conn, table, data, schema?) — insert object; returns {last_insert_id, affected_rows}.
// >>> import "nmysql"
// >>> nmysql.escape_literal("ins")
// => "'ins'"
fn nmysql_insert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nmysql_insert", span)?;
    let id = conn_arg(args, 0, "nmysql_insert", span)?;
    let table = string_arg(args, 1, "nmysql_insert", span)?;
    let data = match &*args[2].borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Ok(nmysql_error(
                span,
                format!(
                    "nmysql_insert() expects data object as argument 3, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let schema = optional_string_arg(args, 3, "nmysql_insert", span)?;
    handles::with_conn_mut(id, "nmysql_insert", span, |handle| {
        insert_on_conn(handle, schema.as_deref(), &table, &data, span)
    })
    .map(|v| v.ref_cell())
    .or_else(|e| Ok(error_from_runtime(&e)))
}

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    vec![
        ("nmysql_connect", Rc::new(nmysql_connect)),
        ("nmysql_connect_opts", Rc::new(nmysql_connect_opts)),
        ("nmysql_close", Rc::new(nmysql_close)),
        ("nmysql_ping", Rc::new(nmysql_ping)),
        ("nmysql_configure", Rc::new(nmysql_configure)),
        ("nmysql_conninfo", Rc::new(nmysql_conninfo)),
        ("nmysql_server_version", Rc::new(nmysql_server_version)),
        ("nmysql_is_in_transaction", Rc::new(nmysql_is_in_transaction)),
        ("nmysql_last_insert_id", Rc::new(nmysql_last_insert_id)),
        ("nmysql_affected_rows", Rc::new(nmysql_affected_rows)),
        ("nmysql_pool", Rc::new(nmysql_pool)),
        ("nmysql_pool_close", Rc::new(nmysql_pool_close)),
        ("nmysql_pool_get", Rc::new(nmysql_pool_get)),
        ("nmysql_pool_status", Rc::new(nmysql_pool_status)),
        ("nmysql_exec", Rc::new(nmysql_exec)),
        ("nmysql_exec_many", Rc::new(nmysql_exec_many)),
        ("nmysql_migrate", Rc::new(nmysql_migrate)),
        ("nmysql_table_exists", Rc::new(nmysql_table_exists)),
        ("nmysql_list_tables", Rc::new(nmysql_list_tables)),
        ("nmysql_table_info", Rc::new(nmysql_table_info)),
        ("nmysql_list_indexes", Rc::new(nmysql_list_indexes)),
        ("nmysql_query", Rc::new(nmysql_query)),
        ("nmysql_query_row", Rc::new(nmysql_query_row)),
        ("nmysql_query_value", Rc::new(nmysql_query_value)),
        ("nmysql_query_column", Rc::new(nmysql_query_column)),
        ("nmysql_prepare", Rc::new(nmysql_prepare)),
        ("nmysql_bind", Rc::new(nmysql_bind)),
        ("nmysql_stmt_exec", Rc::new(nmysql_stmt_exec)),
        ("nmysql_stmt_query", Rc::new(nmysql_stmt_query)),
        ("nmysql_stmt_reset", Rc::new(nmysql_stmt_reset)),
        ("nmysql_finalize", Rc::new(nmysql_finalize)),
        ("nmysql_begin", Rc::new(nmysql_begin)),
        ("nmysql_commit", Rc::new(nmysql_commit)),
        ("nmysql_rollback", Rc::new(nmysql_rollback)),
        ("nmysql_savepoint", Rc::new(nmysql_savepoint)),
        ("nmysql_rollback_to", Rc::new(nmysql_rollback_to)),
        ("nmysql_batch", Rc::new(nmysql_batch)),
        ("nmysql_insert", Rc::new(nmysql_insert)),
        ("nmysql_version", Rc::new(nmysql_version)),
        ("nmysql_escape_literal", Rc::new(nmysql_escape_literal)),
        ("nmysql_quote_ident", Rc::new(nmysql_quote_ident)),
        ("nmysql_async_exec", Rc::new(nmysql_async_exec)),
        ("nmysql_async_query", Rc::new(nmysql_async_query)),
        ("nmysql_task_done", Rc::new(nmysql_task_done)),
        ("nmysql_task_wait", Rc::new(nmysql_task_wait)),
        ("nmysql_task_result", Rc::new(nmysql_task_result)),
        ("nmysql_task_cancel", Rc::new(nmysql_task_cancel)),
    ]
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    let bind = |map: &mut HashMap<String, ValueRef>, name: &str, f: NativeFn| {
        map.insert(name.to_string(), Value::NativeFunction(f).ref_cell());
    };
    bind(&mut map, "connect", Rc::new(nmysql_connect));
    bind(&mut map, "connect_opts", Rc::new(nmysql_connect_opts));
    bind(&mut map, "close", Rc::new(nmysql_close));
    bind(&mut map, "ping", Rc::new(nmysql_ping));
    bind(&mut map, "configure", Rc::new(nmysql_configure));
    bind(&mut map, "conninfo", Rc::new(nmysql_conninfo));
    bind(&mut map, "server_version", Rc::new(nmysql_server_version));
    bind(
        &mut map,
        "is_in_transaction",
        Rc::new(nmysql_is_in_transaction),
    );
    bind(&mut map, "last_insert_id", Rc::new(nmysql_last_insert_id));
    bind(&mut map, "affected_rows", Rc::new(nmysql_affected_rows));
    bind(&mut map, "pool", Rc::new(nmysql_pool));
    bind(&mut map, "pool_close", Rc::new(nmysql_pool_close));
    bind(&mut map, "pool_get", Rc::new(nmysql_pool_get));
    bind(&mut map, "pool_status", Rc::new(nmysql_pool_status));
    bind(&mut map, "exec", Rc::new(nmysql_exec));
    bind(&mut map, "exec_many", Rc::new(nmysql_exec_many));
    bind(&mut map, "migrate", Rc::new(nmysql_migrate));
    bind(&mut map, "table_exists", Rc::new(nmysql_table_exists));
    bind(&mut map, "list_tables", Rc::new(nmysql_list_tables));
    bind(&mut map, "table_info", Rc::new(nmysql_table_info));
    bind(&mut map, "list_indexes", Rc::new(nmysql_list_indexes));
    bind(&mut map, "query", Rc::new(nmysql_query));
    bind(&mut map, "query_row", Rc::new(nmysql_query_row));
    bind(&mut map, "query_value", Rc::new(nmysql_query_value));
    bind(&mut map, "query_column", Rc::new(nmysql_query_column));
    bind(&mut map, "prepare", Rc::new(nmysql_prepare));
    bind(&mut map, "bind", Rc::new(nmysql_bind));
    bind(&mut map, "stmt_exec", Rc::new(nmysql_stmt_exec));
    bind(&mut map, "stmt_query", Rc::new(nmysql_stmt_query));
    bind(&mut map, "stmt_reset", Rc::new(nmysql_stmt_reset));
    bind(&mut map, "finalize", Rc::new(nmysql_finalize));
    bind(&mut map, "begin", Rc::new(nmysql_begin));
    bind(&mut map, "commit", Rc::new(nmysql_commit));
    bind(&mut map, "rollback", Rc::new(nmysql_rollback));
    bind(&mut map, "savepoint", Rc::new(nmysql_savepoint));
    bind(&mut map, "rollback_to", Rc::new(nmysql_rollback_to));
    bind(&mut map, "batch", Rc::new(nmysql_batch));
    bind(&mut map, "insert", Rc::new(nmysql_insert));
    bind(&mut map, "version", Rc::new(nmysql_version));
    bind(&mut map, "escape_literal", Rc::new(nmysql_escape_literal));
    bind(&mut map, "quote_ident", Rc::new(nmysql_quote_ident));
    bind(&mut map, "async_exec", Rc::new(nmysql_async_exec));
    bind(&mut map, "async_query", Rc::new(nmysql_async_query));
    bind(&mut map, "task_done", Rc::new(nmysql_task_done));
    bind(&mut map, "task_wait", Rc::new(nmysql_task_wait));
    bind(&mut map, "task_result", Rc::new(nmysql_task_result));
    bind(&mut map, "task_cancel", Rc::new(nmysql_task_cancel));
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmysql";
pub const MODULE_PATHS: &[&str] = &["nmysql", "std/nmysql"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}
