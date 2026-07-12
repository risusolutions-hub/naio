//! Thread-local handle tables for PostgreSQL connections, pools, and statements.

use niao_db::postgres::Client;
use niao_db::PooledConnection;
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap as StdHashMap;

use super::config::{PgPool, PostgresConnectionManager};

pub type PooledConn = PooledConnection<PostgresConnectionManager>;

pub enum ConnInner {
    Direct(Client),
    Pooled(PooledConn),
}

pub struct ConnHandle {
    pub inner: ConnInner,
    pub conninfo: String,
    pub in_transaction: bool,
}

impl ConnHandle {
    pub fn client_mut(&mut self) -> &mut Client {
        match &mut self.inner {
            ConnInner::Direct(c) => c,
            ConnInner::Pooled(p) => p,
        }
    }
}

pub struct PoolHandle {
    pub pool: PgPool,
    pub conninfo: String,
}

pub struct StmtHandle {
    pub conn_id: u64,
    pub sql: String,
    pub params: Vec<(i32, crate::npg::types::BoundValue)>,
}

thread_local! {
    static NEXT_CONN: RefCell<u64> = RefCell::new(1);
    static NEXT_POOL: RefCell<u64> = RefCell::new(1);
    static NEXT_STMT: RefCell<u64> = RefCell::new(1);
    static CONNS: RefCell<StdHashMap<u64, ConnHandle>> = RefCell::new(StdHashMap::new());
    static POOLS: RefCell<StdHashMap<u64, PoolHandle>> = RefCell::new(StdHashMap::new());
    static STMTS: RefCell<StdHashMap<u64, StmtHandle>> = RefCell::new(StdHashMap::new());
}

pub fn alloc_conn(client: Client, conninfo: String) -> u64 {
    let id = next_conn_id();
    CONNS.with(|m| {
        m.borrow_mut().insert(
            id,
            ConnHandle {
                inner: ConnInner::Direct(client),
                conninfo,
                in_transaction: false,
            },
        );
    });
    id
}

pub fn alloc_pooled_conn(pooled: PooledConn, conninfo: String) -> u64 {
    let id = next_conn_id();
    CONNS.with(|m| {
        m.borrow_mut().insert(
            id,
            ConnHandle {
                inner: ConnInner::Pooled(pooled),
                conninfo,
                in_transaction: false,
            },
        );
    });
    id
}

fn next_conn_id() -> u64 {
    NEXT_CONN.with(|n| {
        let mut next = n.borrow_mut();
        let id = *next;
        *next = id + 1;
        id
    })
}

pub fn remove_conn(id: u64) -> Option<ConnHandle> {
    STMTS.with(|m| {
        m.borrow_mut().retain(|_, stmt| stmt.conn_id != id);
    });
    CONNS.with(|m| m.borrow_mut().remove(&id))
}

pub fn conn_info(id: u64) -> Option<String> {
    CONNS.with(|m| m.borrow().get(&id).map(|c| c.conninfo.clone()))
}

pub fn alloc_pool(pool: PgPool, conninfo: String) -> u64 {
    let id = NEXT_POOL.with(|n| {
        let mut next = n.borrow_mut();
        let id = *next;
        *next = id + 1;
        id
    });
    POOLS.with(|m| {
        m.borrow_mut().insert(id, PoolHandle { pool, conninfo });
    });
    id
}

pub fn remove_pool(id: u64) -> Option<PoolHandle> {
    POOLS.with(|m| m.borrow_mut().remove(&id))
}

pub fn with_pool<F, R>(id: u64, name: &str, span: Span, f: F) -> Result<R, crate::RuntimeError>
where
    F: FnOnce(&PoolHandle) -> Result<R, String>,
{
    POOLS.with(|m| {
        let guard = m.borrow();
        let handle = guard.get(&id).ok_or_else(|| {
            crate::RuntimeError::at(
                span,
                codes::E1902_NPG_INVALID_HANDLE,
                format!("{name}(): invalid or closed pool handle {id}"),
            )
        })?;
        f(handle).map_err(|msg| {
            crate::RuntimeError::at(span, codes::E1901_NPG_ERROR, format!("{name}(): {msg}"))
        })
    })
}

pub fn with_conn_mut<F, R>(id: u64, name: &str, span: Span, f: F) -> Result<R, crate::RuntimeError>
where
    F: FnOnce(&mut ConnHandle) -> Result<R, String>,
{
    CONNS.with(|m| {
        let mut guard = m.borrow_mut();
        let handle = guard.get_mut(&id).ok_or_else(|| {
            crate::RuntimeError::at(
                span,
                codes::E1902_NPG_INVALID_HANDLE,
                format!("{name}(): invalid or closed connection handle {id}"),
            )
        })?;
        f(handle).map_err(|msg| {
            crate::RuntimeError::at(span, codes::E1901_NPG_ERROR, format!("{name}(): {msg}"))
        })
    })
}

pub fn alloc_stmt(conn_id: u64, sql: String) -> u64 {
    let id = NEXT_STMT.with(|n| {
        let mut next = n.borrow_mut();
        let id = *next;
        *next = id + 1;
        id
    });
    STMTS.with(|m| {
        m.borrow_mut().insert(
            id,
            StmtHandle {
                conn_id,
                sql,
                params: Vec::new(),
            },
        );
    });
    id
}

pub fn remove_stmt(id: u64) -> Option<StmtHandle> {
    STMTS.with(|m| m.borrow_mut().remove(&id))
}

pub fn with_stmt_mut<F, R>(id: u64, name: &str, span: Span, f: F) -> Result<R, crate::RuntimeError>
where
    F: FnOnce(&mut StmtHandle) -> Result<R, String>,
{
    STMTS.with(|m| {
        let mut guard = m.borrow_mut();
        let handle = guard.get_mut(&id).ok_or_else(|| {
            crate::RuntimeError::at(
                span,
                codes::E1902_NPG_INVALID_HANDLE,
                format!("{name}(): invalid or finalized statement handle {id}"),
            )
        })?;
        f(handle).map_err(|msg| {
            crate::RuntimeError::at(span, codes::E1901_NPG_ERROR, format!("{name}(): {msg}"))
        })
    })
}

pub fn with_stmt_and_conn<F, R>(stmt_id: u64, name: &str, span: Span, f: F) -> Result<R, crate::RuntimeError>
where
    F: FnOnce(&mut StmtHandle, &mut ConnHandle) -> Result<R, String>,
{
    CONNS.with(|cm| {
        STMTS.with(|sm| {
            let mut cg = cm.borrow_mut();
            let mut sg = sm.borrow_mut();
            let stmt = sg.get_mut(&stmt_id).ok_or_else(|| {
                crate::RuntimeError::at(
                    span,
                    codes::E1902_NPG_INVALID_HANDLE,
                    format!("{name}(): invalid statement handle {stmt_id}"),
                )
            })?;
            let conn_id = stmt.conn_id;
            let conn = cg.get_mut(&conn_id).ok_or_else(|| {
                crate::RuntimeError::at(
                    span,
                    codes::E1902_NPG_INVALID_HANDLE,
                    format!("{name}(): invalid connection handle {conn_id}"),
                )
            })?;
            f(stmt, conn).map_err(|msg| {
                crate::RuntimeError::at(span, codes::E1901_NPG_ERROR, format!("{name}(): {msg}"))
            })
        })
    })
}

pub fn redact_conninfo(s: &str) -> String {
    if let Ok(mut url) = niao_http::parse_url(s) {
        if !url.password.is_empty() {
            url.password = "***".into();
        }
        url.to_string_full()
    } else {
        s.to_string()
    }
}
