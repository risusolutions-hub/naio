//! Error type for `niao_kv` core operations.

use std::fmt;

/// Result alias for store operations.
pub type KvResult<T> = Result<T, KvError>;

/// Errors raised by the embedded store (never panics).
#[derive(Debug, Clone)]
pub enum KvError {
    /// Underlying redb / IO failure.
    Store(String),
    /// Operation requires a write transaction.
    ReadOnly,
    /// Transaction already committed or aborted.
    TxnClosed,
    /// Invalid argument (empty table name, bad range, …).
    Invalid(String),
}

impl fmt::Display for KvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KvError::Store(m) => write!(f, "{m}"),
            KvError::ReadOnly => write!(f, "write operation on a read-only transaction"),
            KvError::TxnClosed => write!(f, "transaction already committed or aborted"),
            KvError::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for KvError {}

impl From<redb::DatabaseError> for KvError {
    fn from(e: redb::DatabaseError) -> Self {
        KvError::Store(e.to_string())
    }
}

impl From<redb::TransactionError> for KvError {
    fn from(e: redb::TransactionError) -> Self {
        KvError::Store(e.to_string())
    }
}

impl From<redb::TableError> for KvError {
    fn from(e: redb::TableError) -> Self {
        KvError::Store(e.to_string())
    }
}

impl From<redb::StorageError> for KvError {
    fn from(e: redb::StorageError) -> Self {
        KvError::Store(e.to_string())
    }
}

impl From<redb::CommitError> for KvError {
    fn from(e: redb::CommitError) -> Self {
        KvError::Store(e.to_string())
    }
}

impl From<std::io::Error> for KvError {
    fn from(e: std::io::Error) -> Self {
        KvError::Store(e.to_string())
    }
}
