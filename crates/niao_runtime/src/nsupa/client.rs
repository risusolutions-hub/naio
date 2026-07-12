//! Supabase client handle registry.
//!
//! Each call to `nsupa_connect` allocates a `SupaClient` record and returns a
//! numeric handle id.  All subsequent API calls carry that id so we can look up
//! the base URL and API keys without re-parsing them every time.

use std::cell::RefCell;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Client record
// ---------------------------------------------------------------------------

pub struct SupaClient {
    /// Supabase project URL, e.g. `https://xyz.supabase.co`
    pub url: String,
    /// Public anonymous key (sent as `apikey` header on every request).
    pub anon_key: String,
    /// Optional service-role key (bypasses RLS).
    pub service_key: Option<String>,
    /// JWT set after `auth_sign_in` / `auth_sign_up`.
    pub auth_token: Option<String>,
}

impl SupaClient {
    /// Returns the effective auth header value: JWT if present, else anon key.
    #[inline]
    pub fn bearer(&self) -> String {
        format!(
            "Bearer {}",
            self.auth_token.as_deref().unwrap_or(&self.anon_key)
        )
    }

    /// Returns `apikey` header value (always the anon key).
    #[inline]
    pub fn api_key(&self) -> &str {
        &self.anon_key
    }
}

// ---------------------------------------------------------------------------
// Thread-local registry
// ---------------------------------------------------------------------------

thread_local! {
    static CLIENTS: RefCell<HashMap<i64, SupaClient>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

/// Insert a new client and return its handle id.
pub fn register(client: SupaClient) -> i64 {
    NEXT_ID.with(|n| {
        let id = *n.borrow();
        *n.borrow_mut() = id + 1;
        CLIENTS.with(|c| c.borrow_mut().insert(id, client));
        id
    })
}

/// Run a closure with a shared reference to a client.
pub fn with_client<F, T>(id: i64, name: &str, _span: niao_ast::Span, f: F) -> Result<T, String>
where
    F: FnOnce(&SupaClient) -> Result<T, String>,
{
    CLIENTS.with(|c| {
        let map = c.borrow();
        match map.get(&id) {
            Some(cl) => f(cl),
            None => Err(format!("{name}: invalid nsupa client handle {id}")),
        }
    })
}

/// Run a closure with a mutable reference to a client (e.g. to store auth token).
pub fn with_client_mut<F, T>(id: i64, name: &str, _span: niao_ast::Span, f: F) -> Result<T, String>
where
    F: FnOnce(&mut SupaClient) -> Result<T, String>,
{
    CLIENTS.with(|c| {
        let mut map = c.borrow_mut();
        match map.get_mut(&id) {
            Some(cl) => f(cl),
            None => Err(format!("{name}: invalid nsupa client handle {id}")),
        }
    })
}

/// Remove a client from the registry (nsupa_close).
pub fn remove(id: i64) -> bool {
    CLIENTS.with(|c| c.borrow_mut().remove(&id).is_some())
}
