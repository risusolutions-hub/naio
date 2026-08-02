//! Thread-local handle table for nredis connections.

use niao_ast::Span;
use niao_db::redis::Client;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static NEXT_ID: RefCell<u64> = const { RefCell::new(1) };
    static CLIENTS: RefCell<HashMap<u64, Client>> = RefCell::new(HashMap::new());
}

/// Allocate a new handle for `client` and return its ID.
pub fn alloc(client: Client) -> u64 {
    let id = NEXT_ID.with(|n| {
        let mut next = n.borrow_mut();
        let id = *next;
        *next = id + 1;
        id
    });
    CLIENTS.with(|m| m.borrow_mut().insert(id, client));
    id
}

/// Remove and return the `Client` for `id`, or `None` if already closed.
pub fn remove(id: u64) -> Option<Client> {
    CLIENTS.with(|m| m.borrow_mut().remove(&id))
}

/// Borrow a `Client` mutably for the duration of `f`.
///
/// Returns `Err(RuntimeError::at(…, E2783))` when the handle is invalid, or
/// `Err(RuntimeError::at(…, E2781))` when the closure returns an error string.
pub fn with_client_mut<F, R>(
    id: u64,
    name: &str,
    span: Span,
    f: F,
) -> Result<R, crate::RuntimeError>
where
    F: FnOnce(&mut Client) -> Result<R, String>,
{
    CLIENTS.with(|m| {
        let mut guard = m.borrow_mut();
        let client = guard.get_mut(&id).ok_or_else(|| {
            crate::RuntimeError::at(
                span,
                codes::E2783_NREDIS_INVALID_HANDLE,
                format!("{name}(): invalid or closed Redis handle {id}"),
            )
        })?;
        f(client).map_err(|msg| {
            crate::RuntimeError::at(span, codes::E2781_NREDIS_ERROR, format!("{name}(): {msg}"))
        })
    })
}
