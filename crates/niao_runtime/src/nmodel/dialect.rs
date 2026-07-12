//! SQL dialect selection for nmodel — SQLite and PostgreSQL backends.

use crate::RuntimeError;
use niao_ast::Span;
use niao_errors::codes;

/// SQL dialect for the nmodel ORM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Pg,
}

impl Dialect {
    /// Returns `?` (SQLite) or `$N` (Pg). `n` is 1-based.
    #[inline]
    pub fn placeholder(self, n: usize) -> String {
        match self {
            Dialect::Sqlite => "?".to_string(),
            Dialect::Pg => format!("${n}"),
        }
    }

    /// SQL type for a boolean field.
    #[inline]
    pub fn bool_type(self) -> &'static str {
        match self {
            Dialect::Sqlite => "INTEGER",
            Dialect::Pg => "BOOLEAN",
        }
    }

    /// SQL type for a datetime field.
    #[inline]
    pub fn datetime_type(self) -> &'static str {
        match self {
            Dialect::Sqlite => "TEXT",
            Dialect::Pg => "TIMESTAMPTZ",
        }
    }

    /// PRIMARY KEY autoincrement declaration for the id column.
    #[inline]
    pub fn autoincrement_pk(self) -> &'static str {
        match self {
            Dialect::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
            Dialect::Pg => "INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY",
        }
    }

    pub fn is_sqlite(self) -> bool {
        self == Dialect::Sqlite
    }

    pub fn is_pg(self) -> bool {
        self == Dialect::Pg
    }
}

pub fn parse_dialect(s: &str, span: Span) -> Result<Dialect, RuntimeError> {
    match s.to_lowercase().as_str() {
        "sqlite" => Ok(Dialect::Sqlite),
        "pg" | "postgres" | "postgresql" => Ok(Dialect::Pg),
        other => Err(RuntimeError::at(
            span,
            codes::E2833_NMODEL_SCHEMA,
            format!("unknown dialect \"{other}\" (use \"sqlite\" or \"pg\")"),
        )),
    }
}
