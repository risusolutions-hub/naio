//! Shared HTTP utilities for the nsupa module.

use crate::{json_parse, Value, ValueRef};
use niao_ast::Span;
use niao_http::{get, post, RequestBuilder};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct HttpError(pub String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Header helpers
// ---------------------------------------------------------------------------

/// Attach `Authorization` and `apikey` headers to any request builder.
#[inline]
pub fn apply_auth_headers(builder: RequestBuilder, bearer: &str, api_key: &str) -> RequestBuilder {
    builder.set("Authorization", bearer).set("apikey", api_key)
}

// ---------------------------------------------------------------------------
// JSON POST helper
// ---------------------------------------------------------------------------

/// POST a JSON body to `url`, returning a parsed Niao value on success.
///
/// `auth_token` is used as `Bearer` when set; otherwise `anon_key` is used.
pub fn http_post_json(
    url: &str,
    anon_key: &str,
    auth_token: Option<&str>,
    body: &str,
) -> Result<crate::ValueRef, HttpError> {
    let bearer = format!("Bearer {}", auth_token.unwrap_or(anon_key));
    let builder = apply_auth_headers(post(url), &bearer, anon_key)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");

    let resp = builder
        .send_string(body)
        .map_err(|e| HttpError(e.to_string()))?;

    if !(200..300).contains(&(resp.status as u16)) {
        return Err(HttpError(format!(
            "HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(&resp.body)
        )));
    }

    let text = String::from_utf8_lossy(&resp.body).into_owned();
    parse_json_response(&text)
}

/// GET request returning a parsed Niao value.
pub fn http_get_json(
    url: &str,
    anon_key: &str,
    auth_token: Option<&str>,
) -> Result<crate::ValueRef, HttpError> {
    let bearer = format!("Bearer {}", auth_token.unwrap_or(anon_key));
    let builder = apply_auth_headers(get(url), &bearer, anon_key).set("Accept", "application/json");

    let resp = builder.send().map_err(|e| HttpError(e.to_string()))?;

    if !(200..300).contains(&(resp.status as u16)) {
        return Err(HttpError(format!(
            "HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(&resp.body)
        )));
    }

    let text = String::from_utf8_lossy(&resp.body).into_owned();
    parse_json_response(&text)
}

/// PATCH request (for UPDATE).
pub fn http_patch_json(
    url: &str,
    anon_key: &str,
    auth_token: Option<&str>,
    body: &str,
    prefer: Option<&str>,
) -> Result<crate::ValueRef, HttpError> {
    let bearer = format!("Bearer {}", auth_token.unwrap_or(anon_key));
    let mut builder = apply_auth_headers(
        niao_http::request(niao_http::Method::Patch, url),
        &bearer,
        anon_key,
    )
    .set("Content-Type", "application/json")
    .set("Accept", "application/json");

    if let Some(pref) = prefer {
        builder = builder.set("Prefer", pref);
    }

    let resp = builder
        .send_string(body)
        .map_err(|e| HttpError(e.to_string()))?;

    if !(200..300).contains(&(resp.status as u16)) {
        return Err(HttpError(format!(
            "HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(&resp.body)
        )));
    }

    let text = String::from_utf8_lossy(&resp.body).into_owned();
    if text.trim().is_empty() {
        return Ok(crate::Value::Bool(true).ref_cell());
    }
    parse_json_response(&text)
}

/// DELETE request.
pub fn http_delete_json(
    url: &str,
    anon_key: &str,
    auth_token: Option<&str>,
) -> Result<crate::ValueRef, HttpError> {
    let bearer = format!("Bearer {}", auth_token.unwrap_or(anon_key));
    let builder = apply_auth_headers(niao_http::delete(url), &bearer, anon_key)
        .set("Accept", "application/json");

    let resp = builder.send().map_err(|e| HttpError(e.to_string()))?;

    if !(200..300).contains(&(resp.status as u16)) {
        return Err(HttpError(format!(
            "HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(&resp.body)
        )));
    }

    Ok(crate::Value::Bool(true).ref_cell())
}

// ---------------------------------------------------------------------------
// JSON parse helper
// ---------------------------------------------------------------------------

pub fn parse_json_response(text: &str) -> Result<ValueRef, HttpError> {
    let arg = Value::String(text.to_string()).ref_cell();
    json_parse(&[arg], Span::dummy()).map_err(|e| HttpError(format!("JSON parse error: {e}")))
}
