//! Schema migrations and introspection.

use super::handles::ConnHandle;
use super::query::exec_on_conn;
use super::types::BoundValue;
use crate::{Value, ValueRef};
use mysql::prelude::*;
use mysql::Row;
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

const MIGRATIONS_TABLE: &str = "_nmysql_schema_version";

pub fn ensure_migrations_table(conn: &mut ConnHandle) -> Result<(), String> {
    exec_on_conn(
        conn,
        &format!(
            "CREATE TABLE IF NOT EXISTS {MIGRATIONS_TABLE} (version BIGINT PRIMARY KEY NOT NULL)"
        ),
        &[],
    )?;
    Ok(())
}

pub fn current_version(conn: &mut ConnHandle) -> Result<i64, String> {
    ensure_migrations_table(conn)?;
    let rows: Vec<Row> = conn
        .client_mut()
        .query(format!(
            "SELECT version FROM {MIGRATIONS_TABLE} ORDER BY version DESC LIMIT 1"
        ))
        .map_err(|e| e.to_string())?;
    if let Some(row) = rows.first() {
        let v = row
            .get::<i64, _>(0)
            .ok_or_else(|| "version column missing".to_string())?;
        Ok(v)
    } else {
        Ok(0)
    }
}

pub fn set_version(conn: &mut ConnHandle, version: i64) -> Result<(), String> {
    exec_on_conn(
        conn,
        &format!("INSERT INTO {MIGRATIONS_TABLE} (version) VALUES (?)"),
        &[BoundValue::Int(version)],
    )?;
    Ok(())
}

fn current_database(conn: &mut ConnHandle) -> Result<String, String> {
    conn.client_mut()
        .query_first::<Option<String>, _>("SELECT DATABASE()")
        .map_err(|e| e.to_string())?
        .flatten()
        .ok_or_else(|| "no current database selected".to_string())
}

pub fn table_exists(conn: &mut ConnHandle, schema: Option<&str>, name: &str) -> Result<bool, String> {
    let schema = match schema {
        Some(s) => s.to_string(),
        None => current_database(conn)?,
    };
    let rows: Vec<Row> = conn
        .client_mut()
        .exec(
            "SELECT 1 FROM information_schema.tables WHERE table_schema = ? AND table_name = ? LIMIT 1",
            (schema, name),
        )
        .map_err(|e| e.to_string())?;
    Ok(!rows.is_empty())
}

pub fn list_tables(conn: &mut ConnHandle, schema: Option<&str>) -> Result<Vec<String>, String> {
    let schema = match schema {
        Some(s) => s.to_string(),
        None => current_database(conn)?,
    };
    let rows: Vec<String> = conn
        .client_mut()
        .exec(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = ? AND table_type = 'BASE TABLE' ORDER BY table_name",
            (schema,),
        )
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn table_info(
    conn: &mut ConnHandle,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<ValueRef>, String> {
    let schema = match schema {
        Some(s) => s.to_string(),
        None => current_database(conn)?,
    };
    let rows: Vec<Row> = conn
        .client_mut()
        .exec(
            "SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = ? AND table_name = ? ORDER BY ordinal_position",
            (schema, table),
        )
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let mut map = HashMap::new();
        map.insert(
            "name".to_string(),
            Value::String(row.get::<String, _>(0).unwrap_or_default()).ref_cell(),
        );
        map.insert(
            "type".to_string(),
            Value::String(row.get::<String, _>(1).unwrap_or_default()).ref_cell(),
        );
        let nullable = row.get::<String, _>(2).unwrap_or_default();
        map.insert(
            "nullable".to_string(),
            Value::Bool(nullable.eq_ignore_ascii_case("YES")).ref_cell(),
        );
        map.insert(
            "default".to_string(),
            match row.get::<Option<String>, _>(3) {
                Some(Some(s)) => Value::String(s).ref_cell(),
                _ => Value::Nil.ref_cell(),
            },
        );
        out.push(Value::Object(map).ref_cell());
    }
    Ok(out)
}

pub fn list_indexes(
    conn: &mut ConnHandle,
    schema: Option<&str>,
    table: Option<&str>,
) -> Result<Vec<ValueRef>, String> {
    let schema = match schema {
        Some(s) => s.to_string(),
        None => current_database(conn)?,
    };
    let rows: Vec<Row> = if let Some(t) = table {
        conn.client_mut()
            .exec(
                "SELECT DISTINCT index_name, table_name, non_unique FROM information_schema.statistics WHERE table_schema = ? AND table_name = ? ORDER BY index_name",
                (schema, t),
            )
            .map_err(|e| e.to_string())?
    } else {
        conn.client_mut()
            .exec(
                "SELECT DISTINCT index_name, table_name, non_unique FROM information_schema.statistics WHERE table_schema = ? ORDER BY index_name",
                (schema,),
            )
            .map_err(|e| e.to_string())?
    };
    let mut out = Vec::new();
    for row in rows {
        let mut map = HashMap::new();
        map.insert(
            "name".to_string(),
            Value::String(row.get::<String, _>(0).unwrap_or_default()).ref_cell(),
        );
        map.insert(
            "table".to_string(),
            Value::String(row.get::<String, _>(1).unwrap_or_default()).ref_cell(),
        );
        let non_unique: i64 = row.get(2).unwrap_or(1);
        map.insert(
            "unique".to_string(),
            Value::Bool(non_unique == 0).ref_cell(),
        );
        out.push(Value::Object(map).ref_cell());
    }
    Ok(out)
}

pub struct Migration {
    pub version: i64,
    pub sql: String,
}

pub fn parse_migrations(
    migrations_ref: &ValueRef,
    span: Span,
) -> Result<Vec<Migration>, crate::RuntimeError> {
    let migrations_val = &*migrations_ref.borrow();
    let items = match migrations_val {
        Value::Array(items) => items,
        other => {
            return Err(crate::RuntimeError::at(
                span,
                codes::E1914_NMYSQL_MIGRATION,
                format!(
                    "nmysql_migrate() expects array of migration objects, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let borrowed = item.borrow();
        let obj = match &*borrowed {
            Value::Object(map) => map,
            other => {
                return Err(crate::RuntimeError::at(
                    span,
                    codes::E1914_NMYSQL_MIGRATION,
                    format!("migration entry must be object, got {}", other.type_name()),
                ));
            }
        };
        let version = obj
            .get("version")
            .ok_or_else(|| {
                crate::RuntimeError::at(
                    span,
                    codes::E1914_NMYSQL_MIGRATION,
                    "migration object missing field \"version\"",
                )
            })
            .and_then(|v| match &*v.borrow() {
                Value::Int(n) => Ok(*n),
                other => Err(crate::RuntimeError::at(
                    span,
                    codes::E1914_NMYSQL_MIGRATION,
                    format!("migration.version must be int, got {}", other.type_name()),
                )),
            })?;
        let sql = obj
            .get("sql")
            .ok_or_else(|| {
                crate::RuntimeError::at(
                    span,
                    codes::E1914_NMYSQL_MIGRATION,
                    "migration object missing field \"sql\"",
                )
            })
            .and_then(|v| match &*v.borrow() {
                Value::String(s) => Ok(s.clone()),
                other => Err(crate::RuntimeError::at(
                    span,
                    codes::E1914_NMYSQL_MIGRATION,
                    format!("migration.sql must be string, got {}", other.type_name()),
                )),
            })?;
        out.push(Migration { version, sql });
    }
    out.sort_by_key(|m| m.version);
    Ok(out)
}

pub fn run_migrations(conn: &mut ConnHandle, migrations: &[Migration]) -> Result<i64, String> {
    ensure_migrations_table(conn)?;
    let mut current = current_version(conn)?;
    let mut applied = 0i64;
    for m in migrations {
        if m.version <= current {
            continue;
        }
        if m.version != current + 1 {
            return Err(format!(
                "expected migration version {}, got {}",
                current + 1,
                m.version
            ));
        }
        for stmt in m.sql.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            exec_on_conn(conn, stmt, &[])?;
        }
        set_version(conn, m.version)?;
        current = m.version;
        applied += 1;
    }
    Ok(applied)
}
