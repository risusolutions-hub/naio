//! GoTrue (Supabase Auth) REST helpers.
//!
//! Endpoints hit:
//!   POST {url}/auth/v1/signup     — nsupa_auth_sign_up
//!   POST {url}/auth/v1/token      — nsupa_auth_sign_in  (grant_type=password)

use super::common::{http_post_json, HttpError};
use crate::Value;

/// Sign up a new user.  Returns the raw JSON response body as a Niao value.
pub fn sign_up(
    base_url: &str,
    anon_key: &str,
    email: &str,
    password: &str,
) -> Result<crate::ValueRef, HttpError> {
    let url = format!("{}/auth/v1/signup", base_url.trim_end_matches('/'));
    let body = format!(
        "{{\"email\":{},\"password\":{}}}",
        json_quote(email),
        json_quote(password)
    );
    http_post_json(&url, anon_key, None, &body)
}

/// Sign in an existing user.  Returns session object; stores `access_token`.
pub fn sign_in(
    base_url: &str,
    anon_key: &str,
    email: &str,
    password: &str,
) -> Result<crate::ValueRef, HttpError> {
    let url = format!(
        "{}/auth/v1/token?grant_type=password",
        base_url.trim_end_matches('/')
    );
    let body = format!(
        "{{\"email\":{},\"password\":{}}}",
        json_quote(email),
        json_quote(password)
    );
    http_post_json(&url, anon_key, None, &body)
}

/// Extract `access_token` string from a session ValueRef returned by sign_in / sign_up.
pub fn extract_access_token(session: &crate::ValueRef) -> Option<String> {
    match &*session.borrow() {
        Value::Object(map) => {
            let tok = map.get("access_token")?;
            match &*tok.borrow() {
                Value::String(s) => Some(s.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Produce a JSON-quoted string (escapes `"` and `\`).
pub fn json_quote(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
