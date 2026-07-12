//! Supabase Storage REST helpers.
//!
//! Endpoints hit:
//!   POST   {url}/storage/v1/object/{bucket}/{path}   — upload (creates or replaces)
//!   GET    {url}/storage/v1/object/{bucket}/{path}   — download

use super::common::{apply_auth_headers, HttpError};
use niao_http::{post, request, Method};

/// Upload a file body (UTF-8 string) to Supabase Storage.
///
/// Returns a Niao value with `{ path }` on success.
pub fn upload(
    base_url: &str,
    bearer: &str,
    api_key: &str,
    bucket: &str,
    path: &str,
    body: &str,
) -> Result<crate::ValueRef, HttpError> {
    let url = format!(
        "{}/storage/v1/object/{}/{}",
        base_url.trim_end_matches('/'),
        bucket,
        path
    );
    let builder = apply_auth_headers(post(&url), bearer, api_key)
        .set("Content-Type", "application/octet-stream");
    let resp = builder
        .send_string(body)
        .map_err(|e| HttpError(e.to_string()))?;

    if !(200..300).contains(&(resp.status as u16)) {
        return Err(HttpError(format!(
            "storage upload HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(&resp.body)
        )));
    }

    // Return { path } object.
    let mut map = std::collections::HashMap::new();
    map.insert(
        "path".to_string(),
        crate::Value::String(format!("{bucket}/{path}")).ref_cell(),
    );
    Ok(crate::Value::Object(map).ref_cell())
}

/// Download a file from Supabase Storage.  Returns the body as a Niao string.
pub fn download(
    base_url: &str,
    bearer: &str,
    api_key: &str,
    bucket: &str,
    path: &str,
) -> Result<crate::ValueRef, HttpError> {
    let url = format!(
        "{}/storage/v1/object/{}/{}",
        base_url.trim_end_matches('/'),
        bucket,
        path
    );
    let builder = apply_auth_headers(niao_http::get(&url), bearer, api_key);
    let resp = builder.send().map_err(|e| HttpError(e.to_string()))?;

    if !(200..300).contains(&(resp.status as u16)) {
        return Err(HttpError(format!(
            "storage download HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(&resp.body)
        )));
    }

    let body = String::from_utf8_lossy(&resp.body).into_owned();
    Ok(crate::Value::String(body).ref_cell())
}
