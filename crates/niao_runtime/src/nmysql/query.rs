//! High-level query helpers and row mapping.

use super::handles::ConnHandle;
use super::types::{bound_to_mysql, mysql_to_niao, rewrite_placeholders, BoundValue};
use crate::Value;
use mysql::prelude::*;
use mysql::Row;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RowFormat {
    Object,
    Array,
}

pub fn parse_row_format(s: &str) -> Result<RowFormat, String> {
    match s {
        "object" => Ok(RowFormat::Object),
        "array" => Ok(RowFormat::Array),
        other => Err(format!("unknown row format \"{other}\" (use \"object\" or \"array\")")),
    }
}

fn column_names(row: &Row) -> Vec<String> {
    row.columns_ref()
        .iter()
        .map(|c| c.name_str().into_owned())
        .collect()
}

fn row_to_object(row: &Row) -> Value {
    let cols = column_names(row);
    let mut map = HashMap::with_capacity(cols.len());
    for (i, name) in cols.into_iter().enumerate() {
        let v = row
            .as_ref(i)
            .cloned()
            .unwrap_or(mysql::Value::NULL);
        map.insert(name, mysql_to_niao(v).ref_cell());
    }
    Value::Object(map)
}

fn row_to_array(row: &Row) -> Value {
    let n = row.len();
    let mut items = Vec::with_capacity(n);
    for i in 0..n {
        let v = row
            .as_ref(i)
            .cloned()
            .unwrap_or(mysql::Value::NULL);
        items.push(mysql_to_niao(v).ref_cell());
    }
    Value::Array(items)
}

pub fn collect_rows(rows: Vec<Row>, format: RowFormat) -> Result<Value, String> {
    if rows.is_empty() {
        return match format {
            RowFormat::Object => Ok(Value::Array(Vec::new())),
            RowFormat::Array => {
                let mut map = HashMap::new();
                map.insert("columns".to_string(), Value::Array(Vec::new()).ref_cell());
                map.insert("rows".to_string(), Value::Array(Vec::new()).ref_cell());
                Ok(Value::Object(map))
            }
        };
    }
    match format {
        RowFormat::Object => {
            let mut out = Vec::with_capacity(rows.len());
            for row in &rows {
                out.push(row_to_object(row).ref_cell());
            }
            Ok(Value::Array(out))
        }
        RowFormat::Array => {
            let cols = column_names(&rows[0]);
            let mut data = Vec::with_capacity(rows.len());
            for row in &rows {
                data.push(row_to_array(row).ref_cell());
            }
            let mut map = HashMap::new();
            map.insert(
                "columns".to_string(),
                Value::Array(cols.into_iter().map(|c| Value::String(c).ref_cell()).collect())
                    .ref_cell(),
            );
            map.insert("rows".to_string(), Value::Array(data).ref_cell());
            Ok(Value::Object(map))
        }
    }
}

pub fn query_on_conn(
    conn: &mut ConnHandle,
    sql: &str,
    params: &[BoundValue],
    format: RowFormat,
) -> Result<Value, String> {
    let sql = rewrite_placeholders(sql);
    let mysql_params = bound_to_mysql(params);
    let rows: Vec<Row> = if mysql_params.is_empty() {
        conn.client_mut()
            .query(sql)
            .map_err(|e| e.to_string())?
    } else {
        conn.client_mut()
            .exec(sql, mysql_params)
            .map_err(|e| e.to_string())?
    };
    conn.refresh_meta();
    collect_rows(rows, format)
}

pub fn query_row_on_conn(
    conn: &mut ConnHandle,
    sql: &str,
    params: &[BoundValue],
) -> Result<Value, String> {
    let sql = rewrite_placeholders(sql);
    let mysql_params = bound_to_mysql(params);
    let rows: Vec<Row> = if mysql_params.is_empty() {
        conn.client_mut()
            .query(sql)
            .map_err(|e| e.to_string())?
    } else {
        conn.client_mut()
            .exec(sql, mysql_params)
            .map_err(|e| e.to_string())?
    };
    conn.refresh_meta();
    if let Some(row) = rows.first() {
        Ok(row_to_object(row))
    } else {
        Ok(Value::Nil)
    }
}

pub fn query_value_on_conn(
    conn: &mut ConnHandle,
    sql: &str,
    params: &[BoundValue],
) -> Result<Value, String> {
    let sql = rewrite_placeholders(sql);
    let mysql_params = bound_to_mysql(params);
    let rows: Vec<Row> = if mysql_params.is_empty() {
        conn.client_mut()
            .query(sql)
            .map_err(|e| e.to_string())?
    } else {
        conn.client_mut()
            .exec(sql, mysql_params)
            .map_err(|e| e.to_string())?
    };
    conn.refresh_meta();
    if let Some(row) = rows.first() {
        let v = row
            .as_ref(0)
            .cloned()
            .unwrap_or(mysql::Value::NULL);
        Ok(mysql_to_niao(v))
    } else {
        Ok(Value::Nil)
    }
}

pub fn query_column_on_conn(
    conn: &mut ConnHandle,
    sql: &str,
    params: &[BoundValue],
) -> Result<Value, String> {
    let sql = rewrite_placeholders(sql);
    let mysql_params = bound_to_mysql(params);
    let rows: Vec<Row> = if mysql_params.is_empty() {
        conn.client_mut()
            .query(sql)
            .map_err(|e| e.to_string())?
    } else {
        conn.client_mut()
            .exec(sql, mysql_params)
            .map_err(|e| e.to_string())?
    };
    conn.refresh_meta();
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        let v = row
            .as_ref(0)
            .cloned()
            .unwrap_or(mysql::Value::NULL);
        out.push(mysql_to_niao(v).ref_cell());
    }
    Ok(Value::Array(out))
}

pub fn exec_on_conn(conn: &mut ConnHandle, sql: &str, params: &[BoundValue]) -> Result<u64, String> {
    let sql = rewrite_placeholders(sql);
    let mysql_params = bound_to_mysql(params);
    if mysql_params.is_empty() {
        conn.client_mut()
            .query_drop(sql)
            .map_err(|e| e.to_string())?;
    } else {
        conn.client_mut()
            .exec_drop(sql, mysql_params)
            .map_err(|e| e.to_string())?;
    }
    conn.refresh_meta();
    Ok(conn.affected_rows)
}

pub fn batch_on_conn(
    conn: &mut ConnHandle,
    sql: &str,
    rows: &[Vec<BoundValue>],
) -> Result<u64, String> {
    let sql = rewrite_placeholders(sql);
    conn.client_mut()
        .query_drop("START TRANSACTION")
        .map_err(|e| e.to_string())?;
    let mut total = 0u64;
    let result = (|| {
        for row in rows {
            let mysql_params = bound_to_mysql(row);
            conn.client_mut()
                .exec_drop(sql.as_str(), mysql_params)
                .map_err(|e| e.to_string())?;
            conn.refresh_meta();
            total += conn.affected_rows;
        }
        conn.client_mut()
            .query_drop("COMMIT")
            .map_err(|e| e.to_string())?;
        Ok(total)
    })();
    if result.is_err() {
        let _ = conn.client_mut().query_drop("ROLLBACK");
    }
    result
}

pub fn insert_on_conn(
    conn: &mut ConnHandle,
    schema: Option<&str>,
    table: &str,
    data: &HashMap<String, crate::ValueRef>,
    span: niao_ast::Span,
) -> Result<Value, String> {
    if data.is_empty() {
        return Err("insert data object is empty".into());
    }
    let table_ref = match schema {
        Some(s) => format!(
            "{}.{}",
            super::types::quote_ident(s),
            super::types::quote_ident(table)
        ),
        None => super::types::quote_ident(table),
    };
    let mut cols = Vec::new();
    let mut placeholders = Vec::new();
    let mut params = Vec::new();
    for (k, v) in data {
        cols.push(super::types::quote_ident(k));
        placeholders.push("?".to_string());
        params.push(
            super::types::niao_to_bound(&*v.borrow(), span).map_err(|e| format!("{e}"))?,
        );
    }
    let sql = format!(
        "INSERT INTO {table_ref} ({}) VALUES ({})",
        cols.join(", "),
        placeholders.join(", ")
    );
    let n = exec_on_conn(conn, &sql, &params)?;
    let mut map = HashMap::new();
    map.insert(
        "last_insert_id".to_string(),
        Value::Int(conn.last_insert_id as i64).ref_cell(),
    );
    map.insert("affected_rows".to_string(), Value::Int(n as i64).ref_cell());
    Ok(Value::Object(map))
}
