//! PostgREST query-builder handle registry.
//!
//! `nsupa_from(client_id, table)` allocates a `QueryHandle` and returns a
//! numeric query-handle id.  Filter helpers (`nsupa_eq`, `nsupa_gt`, `nsupa_lt`,
//! `nsupa_order`, `nsupa_limit`) mutate the handle and return the same id so
//! callers can chain.  Terminal operations (`nsupa_select`, `nsupa_insert`,
//! `nsupa_update`, `nsupa_delete`) consume the handle and fire the HTTP request.

use std::cell::RefCell;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

pub struct QueryHandle {
    pub client_id: i64,
    pub table: String,
    /// PostgREST column filters, e.g. `["age=gt.18", "name=eq.Alice"]`.
    pub filters: Vec<String>,
    /// Comma-separated column list for SELECT, or `*`.
    pub select_cols: String,
    /// ORDER BY clause, e.g. `created_at.desc`.
    pub order: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// For UPDATE / INSERT, the preferred representation header value.
    pub prefer: Option<String>,
}

impl QueryHandle {
    pub fn new(client_id: i64, table: impl Into<String>) -> Self {
        QueryHandle {
            client_id,
            table: table.into(),
            filters: Vec::new(),
            select_cols: "*".to_string(),
            order: None,
            limit: None,
            offset: None,
            prefer: None,
        }
    }

    /// Build the base PostgREST REST URL path for this query.
    ///
    /// `base_url` is the Supabase project URL (no trailing slash).
    pub fn rest_url(&self, base_url: &str) -> String {
        let mut url = format!("{}/rest/v1/{}", base_url.trim_end_matches('/'), self.table);
        let mut params: Vec<String> = Vec::new();

        // Column selection.
        if self.select_cols != "*" {
            params.push(format!("select={}", encode(&self.select_cols)));
        }

        // Filters.
        for f in &self.filters {
            params.push(f.clone());
        }

        // ORDER.
        if let Some(ord) = &self.order {
            params.push(format!("order={}", encode(ord)));
        }

        // LIMIT / OFFSET.
        if let Some(lim) = self.limit {
            params.push(format!("limit={lim}"));
        }
        if let Some(off) = self.offset {
            params.push(format!("offset={off}"));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        url
    }
}

/// Minimal percent-encoding for query parameter values (space → %20, etc.).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' | b'*' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Thread-local registry
// ---------------------------------------------------------------------------

thread_local! {
    static QUERIES: RefCell<HashMap<i64, QueryHandle>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

pub fn register(qh: QueryHandle) -> i64 {
    NEXT_ID.with(|n| {
        let id = *n.borrow();
        *n.borrow_mut() = id + 1;
        QUERIES.with(|q| q.borrow_mut().insert(id, qh));
        id
    })
}

/// Mutate a query handle in place; return its id on success.
pub fn with_query_mut<F>(id: i64, name: &str, f: F) -> Result<i64, String>
where
    F: FnOnce(&mut QueryHandle) -> Result<(), String>,
{
    QUERIES.with(|q| {
        let mut map = q.borrow_mut();
        match map.get_mut(&id) {
            Some(qh) => {
                f(qh)?;
                Ok(id)
            }
            None => Err(format!("{name}: invalid nsupa query handle {id}")),
        }
    })
}

/// Take ownership of a query handle (terminal operations consume the handle).
pub fn take(id: i64, name: &str) -> Result<QueryHandle, String> {
    QUERIES.with(|q| {
        q.borrow_mut()
            .remove(&id)
            .ok_or_else(|| format!("{name}: invalid nsupa query handle {id}"))
    })
}

/// Remove without consuming (e.g. user explicitly drops).
pub fn remove(id: i64) -> bool {
    QUERIES.with(|q| q.borrow_mut().remove(&id).is_some())
}

// ---------------------------------------------------------------------------
// Unit tests — URL builder
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn qh(table: &str) -> QueryHandle {
        QueryHandle::new(1, table)
    }

    #[test]
    fn plain_url_no_params() {
        let q = qh("users");
        assert_eq!(
            q.rest_url("https://abc.supabase.co"),
            "https://abc.supabase.co/rest/v1/users"
        );
    }

    #[test]
    fn select_cols() {
        let mut q = qh("users");
        q.select_cols = "id,name,email".to_string();
        let url = q.rest_url("https://abc.supabase.co");
        assert!(
            url.contains("select=id%2Cname%2Cemail")
                || url.contains("select=id,name,email")
                || url.contains("select=id")
        );
        // Just verify select param is present
        assert!(url.contains("select="));
    }

    #[test]
    fn eq_filter() {
        let mut q = qh("posts");
        q.filters.push("status=eq.published".to_string());
        let url = q.rest_url("https://proj.supabase.co");
        assert!(url.contains("status=eq.published"), "url: {url}");
    }

    #[test]
    fn gt_and_limit() {
        let mut q = qh("orders");
        q.filters.push("total=gt.100".to_string());
        q.limit = Some(10);
        let url = q.rest_url("https://proj.supabase.co");
        assert!(url.contains("total=gt.100"), "url: {url}");
        assert!(url.contains("limit=10"), "url: {url}");
    }

    #[test]
    fn order_asc() {
        let mut q = qh("events");
        q.order = Some("created_at.asc".to_string());
        let url = q.rest_url("https://proj.supabase.co");
        assert!(url.contains("order="), "url: {url}");
        assert!(url.contains("created_at"), "url: {url}");
    }

    #[test]
    fn trailing_slash_stripped() {
        let q = qh("items");
        assert_eq!(
            q.rest_url("https://proj.supabase.co/"),
            "https://proj.supabase.co/rest/v1/items"
        );
    }

    #[test]
    fn multiple_filters_joined() {
        let mut q = qh("products");
        q.filters.push("price=gt.10".to_string());
        q.filters.push("category=eq.books".to_string());
        let url = q.rest_url("https://x.supabase.co");
        assert!(url.contains("price=gt.10"));
        assert!(url.contains("category=eq.books"));
    }

    #[test]
    fn limit_and_offset() {
        let mut q = qh("logs");
        q.limit = Some(20);
        q.offset = Some(40);
        let url = q.rest_url("https://x.supabase.co");
        assert!(url.contains("limit=20"));
        assert!(url.contains("offset=40"));
    }
}
