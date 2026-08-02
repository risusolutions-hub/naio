//! Migration runner for nmodel.
//!
//! Tracks applied models in `_nmodel_migrations`. On each `migrate()` call,
//! any model not yet present gets its `CREATE TABLE` executed and is recorded.

use super::dialect::Dialect;
use super::schema::{create_table_sql, SchemaHandle};
use crate::RuntimeError;
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashSet;

pub const MIGRATIONS_TABLE: &str = "_nmodel_migrations";

// ── SQLite ─────────────────────────────────────────────────────────────────

/// Run pending model migrations against a SQLite connection handle.
/// Returns the number of newly-applied model tables.
pub fn sqlite_migrate(
    conn: &mut crate::nsqlite::handles::ConnHandle,
    schema: &SchemaHandle,
    _dialect: Dialect,
) -> Result<i64, String> {
    sqlite_ensure_table(conn)?;
    let applied = sqlite_get_applied(conn)?;
    let mut count = 0i64;
    for (model_name, model_def) in &schema.models {
        if applied.contains(model_name.as_str()) {
            continue;
        }
        let ddl = create_table_sql(model_def, Dialect::Sqlite);
        conn.conn.execute(&ddl, []).map_err(|e| e.to_string())?;
        let ins = format!(
            "INSERT INTO \"{}\" (model_name) VALUES (?1)",
            MIGRATIONS_TABLE
        );
        conn.conn
            .execute(&ins, [model_name.as_str()])
            .map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
}

fn sqlite_ensure_table(conn: &mut crate::nsqlite::handles::ConnHandle) -> Result<(), String> {
    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (\
          model_name TEXT PRIMARY KEY NOT NULL,\
          applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
        )",
        MIGRATIONS_TABLE
    );
    conn.conn.execute_batch(&ddl).map_err(|e| e.to_string())
}

fn sqlite_get_applied(
    conn: &mut crate::nsqlite::handles::ConnHandle,
) -> Result<HashSet<String>, String> {
    let sql = format!("SELECT model_name FROM \"{}\"", MIGRATIONS_TABLE);
    let mut stmt = conn.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt.raw_query();
    let mut set = HashSet::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        set.insert(row.get::<_, String>(0).map_err(|e| e.to_string())?);
    }
    Ok(set)
}

// ── PostgreSQL ─────────────────────────────────────────────────────────────

/// Run pending model migrations against a PostgreSQL connection handle.
pub fn pg_migrate(
    conn: &mut crate::npg::handles::ConnHandle,
    schema: &SchemaHandle,
) -> Result<i64, String> {
    pg_ensure_table(conn)?;
    let applied = pg_get_applied(conn)?;
    let mut count = 0i64;
    for (model_name, model_def) in &schema.models {
        if applied.contains(model_name.as_str()) {
            continue;
        }
        let ddl = create_table_sql(model_def, Dialect::Pg);
        conn.client_mut()
            .batch_execute(&ddl)
            .map_err(|e| e.to_string())?;
        // Inline model_name (safe: model names are user-controlled identifiers,
        // not arbitrary SQL).  No parameterised INSERT to avoid ToSql coupling.
        let safe_name = model_name.replace('\'', "''");
        let ins = format!(
            "INSERT INTO \"{}\" (model_name) VALUES ('{}')",
            MIGRATIONS_TABLE, safe_name
        );
        conn.client_mut()
            .batch_execute(&ins)
            .map_err(|e| e.to_string())?;
        count += 1;
    }
    Ok(count)
}

fn pg_ensure_table(conn: &mut crate::npg::handles::ConnHandle) -> Result<(), String> {
    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (\
          model_name TEXT PRIMARY KEY NOT NULL,\
          applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()\
        )",
        MIGRATIONS_TABLE
    );
    conn.client_mut()
        .batch_execute(&ddl)
        .map_err(|e| e.to_string())
}

fn pg_get_applied(conn: &mut crate::npg::handles::ConnHandle) -> Result<HashSet<String>, String> {
    let sql = format!("SELECT model_name FROM \"{}\"", MIGRATIONS_TABLE);
    let rows = conn
        .client_mut()
        .query(sql.as_str(), &[])
        .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| r.get::<_, String>(0)).collect())
}

// ── Error helper ───────────────────────────────────────────────────────────

pub fn migration_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E2831_NMODEL_ERROR, msg.into())
}
