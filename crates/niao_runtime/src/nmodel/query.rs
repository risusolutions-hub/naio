//! CRUD query generation and execution for nmodel.
//!
//! All SQL uses `?` placeholders; the PostgreSQL path rewrites them to `$N`
//! automatically via `npg::query::exec_on_conn` / `query_on_conn`.

use super::dialect::Dialect;
use super::schema::ModelDef;
use crate::{RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

// ── Common error helper ────────────────────────────────────────────────────

pub fn nmodel_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2831_NMODEL_ERROR, msg.into())
}

pub fn nmodel_schema_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2833_NMODEL_SCHEMA, msg.into())
}

// ── WHERE clause builder ───────────────────────────────────────────────────

/// Parse a `where` object from Niao (`{col: val, ...}`) into a WHERE clause
/// string + collected `Value` params (appended to `params`).
pub fn build_where(
    where_obj: &HashMap<String, ValueRef>,
    params: &mut Vec<Value>,
    dialect: Dialect,
) -> String {
    if where_obj.is_empty() {
        return String::new();
    }
    let mut clauses: Vec<String> = Vec::with_capacity(where_obj.len());
    for (col, val) in where_obj {
        let ph = dialect.placeholder(params.len() + 1);
        clauses.push(format!("\"{}\" = {}", col, ph));
        params.push((*val.borrow()).clone());
    }
    format!("WHERE {}", clauses.join(" AND "))
}

/// Extract the `where` sub-object from a query options object.
pub fn extract_where(
    opts: &HashMap<String, ValueRef>,
    span: Span,
) -> Result<HashMap<String, ValueRef>, RuntimeError> {
    match opts.get("where") {
        None => Ok(HashMap::new()),
        Some(v) => match &*v.borrow() {
            Value::Object(m) => Ok(m.clone()),
            other => Err(nmodel_schema_err(
                span,
                format!("query \"where\" must be object, got {}", other.type_name()),
            )),
        },
    }
}

// ── Value → SQLite BoundValue ──────────────────────────────────────────────

fn values_to_sqlite_params(
    vals: &[Value],
    span: Span,
) -> Result<Vec<crate::nsqlite::types::BoundValue>, RuntimeError> {
    vals.iter()
        .map(|v| crate::nsqlite::types::niao_to_bound(v, span))
        .collect()
}

// ── Value → Pg BoundValue ─────────────────────────────────────────────────

fn values_to_pg_params(
    vals: &[Value],
    span: Span,
) -> Result<Vec<crate::npg::types::BoundValue>, RuntimeError> {
    vals.iter()
        .map(|v| crate::npg::types::niao_to_bound(v, span))
        .collect()
}

// ── SQLite row helpers ─────────────────────────────────────────────────────

fn sqlite_query_rows(
    conn: &mut crate::nsqlite::handles::ConnHandle,
    sql: &str,
    params: &[crate::nsqlite::types::BoundValue],
) -> Result<Value, String> {
    let mut stmt = conn.conn.prepare(sql).map_err(|e| e.to_string())?;
    let col_count = stmt.column_count();
    let cols: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("").to_string())
        .collect();
    for (i, p) in params.iter().enumerate() {
        crate::nsqlite::types::bind_positional(&mut stmt, (i + 1) as i32, p)?;
    }
    let mut iter = stmt.raw_query();
    let mut out: Vec<ValueRef> = Vec::new();
    while let Some(row) = iter.next().map_err(|e| e.to_string())? {
        let mut map: HashMap<String, ValueRef> = HashMap::with_capacity(col_count);
        for (i, name) in cols.iter().enumerate() {
            let val = row
                .get::<_, rusqlite::types::Value>(i)
                .unwrap_or(rusqlite::types::Value::Null);
            map.insert(
                name.clone(),
                crate::nsqlite::types::sql_to_niao(val).ref_cell(),
            );
        }
        out.push(Value::Object(map).ref_cell());
    }
    Ok(Value::Array(out))
}

fn sqlite_exec(
    conn: &mut crate::nsqlite::handles::ConnHandle,
    sql: &str,
    params: &[crate::nsqlite::types::BoundValue],
) -> Result<i64, String> {
    let mut stmt = conn.conn.prepare(sql).map_err(|e| e.to_string())?;
    for (i, p) in params.iter().enumerate() {
        crate::nsqlite::types::bind_positional(&mut stmt, (i + 1) as i32, p)?;
    }
    stmt.raw_execute()
        .map(|n| n as i64)
        .map_err(|e| e.to_string())
}

// ── CREATE ─────────────────────────────────────────────────────────────────

/// Execute INSERT + return the created row.
pub fn exec_create(
    model: &ModelDef,
    data: &HashMap<String, ValueRef>,
    db_handle: u64,
    dialect: Dialect,
    span: Span,
) -> Result<Value, RuntimeError> {
    if data.is_empty() {
        return Err(nmodel_err(
            span,
            "nmodel.create() data object must not be empty",
        ));
    }
    let mut cols: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    for (col, val) in data {
        cols.push(format!("\"{}\"", col));
        params.push((*val.borrow()).clone());
    }

    let placeholders: Vec<String> = (1..=params.len()).map(|i| dialect.placeholder(i)).collect();

    match dialect {
        Dialect::Sqlite => {
            let sql = format!(
                "INSERT INTO \"{}\" ({}) VALUES ({})",
                model.name,
                cols.join(", "),
                placeholders.join(", ")
            );
            let bound = values_to_sqlite_params(&params, span)?;
            let row =
                crate::nsqlite::handles::with_conn_mut(db_handle, "nmodel_create", span, |conn| {
                    sqlite_exec(conn, &sql, &bound)?;
                    let id = conn.conn.last_insert_rowid();
                    let id_col = model.id_field().map(|f| f.name.as_str()).unwrap_or("id");
                    let sel = format!("SELECT * FROM \"{}\" WHERE \"{}\" = ?1", model.name, id_col);
                    let sel_params = [crate::nsqlite::types::BoundValue::Int(id)];
                    sqlite_query_rows(conn, &sel, &sel_params)
                })
                .map_err(|e| nmodel_err(span, e.to_string()))?;

            if let Value::Array(rows) = row {
                Ok(rows
                    .into_iter()
                    .next()
                    .map(|r| (*r.borrow()).clone())
                    .unwrap_or(Value::Nil))
            } else {
                Ok(Value::Nil)
            }
        }
        Dialect::Pg => {
            let sql = format!(
                "INSERT INTO \"{}\" ({}) VALUES ({}) RETURNING *",
                model.name,
                cols.join(", "),
                placeholders.join(", ")
            );
            let bound = values_to_pg_params(&params, span)?;
            crate::npg::handles::with_conn_mut(db_handle, "nmodel_create", span, |conn| {
                crate::npg::query::query_row_on_conn(conn, &sql, &bound)
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
    }
}

// ── FIND MANY ──────────────────────────────────────────────────────────────

pub struct FindManyOpts {
    pub where_obj: HashMap<String, ValueRef>,
    pub limit: Option<i64>,
    pub order: Option<String>,
}

pub fn parse_find_many_opts(
    opts: &HashMap<String, ValueRef>,
    span: Span,
) -> Result<FindManyOpts, RuntimeError> {
    let where_obj = extract_where(opts, span)?;
    let limit = match opts.get("limit") {
        None => None,
        Some(v) => match &*v.borrow() {
            Value::Int(n) => Some(*n),
            other => {
                return Err(nmodel_schema_err(
                    span,
                    format!("\"limit\" must be int, got {}", other.type_name()),
                ))
            }
        },
    };
    let order = match opts.get("order") {
        None => None,
        Some(v) => match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            other => {
                return Err(nmodel_schema_err(
                    span,
                    format!("\"order\" must be string, got {}", other.type_name()),
                ))
            }
        },
    };
    Ok(FindManyOpts {
        where_obj,
        limit,
        order,
    })
}

pub fn exec_find_many(
    model: &ModelDef,
    opts: FindManyOpts,
    db_handle: u64,
    dialect: Dialect,
    span: Span,
) -> Result<Value, RuntimeError> {
    let mut params: Vec<Value> = Vec::new();
    let where_clause = build_where(&opts.where_obj, &mut params, dialect);

    let mut sql = format!("SELECT * FROM \"{}\"", model.name);
    if !where_clause.is_empty() {
        sql.push(' ');
        sql.push_str(&where_clause);
    }
    if let Some(ref order) = opts.order {
        sql.push_str(&format!(" ORDER BY {}", order));
    }
    if let Some(lim) = opts.limit {
        let ph = dialect.placeholder(params.len() + 1);
        sql.push_str(&format!(" LIMIT {}", ph));
        params.push(Value::Int(lim));
    }

    match dialect {
        Dialect::Sqlite => {
            let bound = values_to_sqlite_params(&params, span)?;
            crate::nsqlite::handles::with_conn_mut(db_handle, "nmodel_find_many", span, |conn| {
                sqlite_query_rows(conn, &sql, &bound)
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
        Dialect::Pg => {
            let bound = values_to_pg_params(&params, span)?;
            crate::npg::handles::with_conn_mut(db_handle, "nmodel_find_many", span, |conn| {
                crate::npg::query::query_on_conn(
                    conn,
                    &sql,
                    &bound,
                    crate::npg::query::RowFormat::Object,
                )
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
    }
}

// ── FIND UNIQUE ────────────────────────────────────────────────────────────

pub fn exec_find_unique(
    model: &ModelDef,
    where_obj: HashMap<String, ValueRef>,
    db_handle: u64,
    dialect: Dialect,
    span: Span,
) -> Result<Value, RuntimeError> {
    let mut params: Vec<Value> = Vec::new();
    let where_clause = build_where(&where_obj, &mut params, dialect);
    let mut sql = format!("SELECT * FROM \"{}\"", model.name);
    if !where_clause.is_empty() {
        sql.push(' ');
        sql.push_str(&where_clause);
    }
    sql.push_str(&format!(" LIMIT {}", dialect.placeholder(params.len() + 1)));
    params.push(Value::Int(1));

    match dialect {
        Dialect::Sqlite => {
            let bound = values_to_sqlite_params(&params, span)?;
            crate::nsqlite::handles::with_conn_mut(db_handle, "nmodel_find_unique", span, |conn| {
                let rows = sqlite_query_rows(conn, &sql, &bound)?;
                if let Value::Array(mut arr) = rows {
                    if arr.is_empty() {
                        Ok(Value::Nil)
                    } else {
                        Ok((*arr.remove(0).borrow()).clone())
                    }
                } else {
                    Ok(Value::Nil)
                }
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
        Dialect::Pg => {
            let bound = values_to_pg_params(&params, span)?;
            crate::npg::handles::with_conn_mut(db_handle, "nmodel_find_unique", span, |conn| {
                crate::npg::query::query_row_on_conn(conn, &sql, &bound)
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
    }
}

// ── UPDATE ─────────────────────────────────────────────────────────────────

pub fn exec_update(
    model: &ModelDef,
    where_obj: HashMap<String, ValueRef>,
    data: HashMap<String, ValueRef>,
    db_handle: u64,
    dialect: Dialect,
    span: Span,
) -> Result<Value, RuntimeError> {
    if data.is_empty() {
        return Err(nmodel_err(
            span,
            "nmodel.update() data object must not be empty",
        ));
    }
    let mut params: Vec<Value> = Vec::new();
    let mut set_parts: Vec<String> = Vec::new();
    for (col, val) in &data {
        let ph = dialect.placeholder(params.len() + 1);
        set_parts.push(format!("\"{}\" = {}", col, ph));
        params.push((*val.borrow()).clone());
    }
    let where_clause = build_where(&where_obj, &mut params, dialect);

    match dialect {
        Dialect::Sqlite => {
            let sql = format!(
                "UPDATE \"{}\" SET {} {}",
                model.name,
                set_parts.join(", "),
                where_clause
            );
            let bound = values_to_sqlite_params(&params, span)?;
            crate::nsqlite::handles::with_conn_mut(db_handle, "nmodel_update", span, |conn| {
                sqlite_exec(conn, &sql, &bound)?;
                // Re-fetch using the where clause
                let mut sel_params: Vec<Value> = Vec::new();
                let sel_where = build_where(&where_obj, &mut sel_params, Dialect::Sqlite);
                let sel = format!("SELECT * FROM \"{}\" {} LIMIT 1", model.name, sel_where);
                let sel_bound =
                    values_to_sqlite_params(&sel_params, span).map_err(|e| e.to_string())?;
                let rows = sqlite_query_rows(conn, &sel, &sel_bound)?;
                if let Value::Array(mut arr) = rows {
                    if arr.is_empty() {
                        Ok(Value::Nil)
                    } else {
                        Ok((*arr.remove(0).borrow()).clone())
                    }
                } else {
                    Ok(Value::Nil)
                }
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
        Dialect::Pg => {
            let sql = format!(
                "UPDATE \"{}\" SET {} {} RETURNING *",
                model.name,
                set_parts.join(", "),
                where_clause
            );
            let bound = values_to_pg_params(&params, span)?;
            crate::npg::handles::with_conn_mut(db_handle, "nmodel_update", span, |conn| {
                crate::npg::query::query_row_on_conn(conn, &sql, &bound)
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
    }
}

// ── DELETE ─────────────────────────────────────────────────────────────────

pub fn exec_delete(
    model: &ModelDef,
    where_obj: HashMap<String, ValueRef>,
    db_handle: u64,
    dialect: Dialect,
    span: Span,
) -> Result<i64, RuntimeError> {
    let mut params: Vec<Value> = Vec::new();
    let where_clause = build_where(&where_obj, &mut params, dialect);
    let sql = format!("DELETE FROM \"{}\" {}", model.name, where_clause);

    match dialect {
        Dialect::Sqlite => {
            let bound = values_to_sqlite_params(&params, span)?;
            crate::nsqlite::handles::with_conn_mut(db_handle, "nmodel_delete", span, |conn| {
                sqlite_exec(conn, &sql, &bound)
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
        Dialect::Pg => {
            let bound = values_to_pg_params(&params, span)?;
            crate::npg::handles::with_conn_mut(db_handle, "nmodel_delete", span, |conn| {
                crate::npg::query::exec_on_conn(conn, &sql, &bound).map(|n| n as i64)
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
    }
}

// ── RAW ────────────────────────────────────────────────────────────────────

/// Execute a raw SQL string. Returns rows[] if it looks like a SELECT, else
/// returns the affected-row count as int.
pub fn exec_raw(
    sql: &str,
    raw_params: &[ValueRef],
    db_handle: u64,
    dialect: Dialect,
    span: Span,
) -> Result<Value, RuntimeError> {
    let params: Vec<Value> = raw_params.iter().map(|v| (*v.borrow()).clone()).collect();
    let is_select = sql.trim_start().to_uppercase().starts_with("SELECT");

    match dialect {
        Dialect::Sqlite => {
            let bound = values_to_sqlite_params(&params, span)?;
            crate::nsqlite::handles::with_conn_mut(db_handle, "nmodel_raw", span, |conn| {
                if is_select {
                    sqlite_query_rows(conn, sql, &bound)
                } else {
                    sqlite_exec(conn, sql, &bound).map(Value::Int)
                }
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
        Dialect::Pg => {
            let bound = values_to_pg_params(&params, span)?;
            crate::npg::handles::with_conn_mut(db_handle, "nmodel_raw", span, |conn| {
                if is_select {
                    crate::npg::query::query_on_conn(
                        conn,
                        sql,
                        &bound,
                        crate::npg::query::RowFormat::Object,
                    )
                } else {
                    crate::npg::query::exec_on_conn(conn, sql, &bound).map(|n| Value::Int(n as i64))
                }
            })
            .map_err(|e| nmodel_err(span, e.to_string()))
        }
    }
}

// ── Opts extraction helpers ────────────────────────────────────────────────

/// Parse `{where: {...}, data: {...}}` for update calls.
pub fn parse_update_opts(
    opts: &HashMap<String, ValueRef>,
    span: Span,
) -> Result<(HashMap<String, ValueRef>, HashMap<String, ValueRef>), RuntimeError> {
    let where_obj = extract_where(opts, span)?;
    let data = match opts.get("data") {
        None => {
            return Err(nmodel_schema_err(
                span,
                "nmodel.update() options missing \"data\" key",
            ))
        }
        Some(v) => match &*v.borrow() {
            Value::Object(m) => m.clone(),
            other => {
                return Err(nmodel_schema_err(
                    span,
                    format!("\"data\" must be object, got {}", other.type_name()),
                ))
            }
        },
    };
    Ok((where_obj, data))
}

// ── SQL generation test ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    #[test]
    fn build_where_sqlite_empty() {
        let mut params = Vec::new();
        let clause = build_where(&HashMap::new(), &mut params, Dialect::Sqlite);
        assert!(clause.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn build_where_sqlite_single() {
        let mut where_obj = HashMap::new();
        where_obj.insert("id".to_string(), Value::Int(1).ref_cell());
        let mut params = Vec::new();
        let clause = build_where(&where_obj, &mut params, Dialect::Sqlite);
        assert!(clause.contains("WHERE"));
        assert!(clause.contains("\"id\" = ?"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn build_where_pg_single() {
        let mut where_obj = HashMap::new();
        where_obj.insert(
            "email".to_string(),
            Value::String("a@b.com".into()).ref_cell(),
        );
        let mut params = Vec::new();
        let clause = build_where(&where_obj, &mut params, Dialect::Pg);
        assert!(clause.contains("$1"));
        assert_eq!(params.len(), 1);
    }
}
