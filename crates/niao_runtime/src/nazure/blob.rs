//! Azure Blob Storage REST operations.
//!
//! Endpoint: `https://{account}.blob.core.windows.net`
//! API version: 2020-08-04
//!
//! Auth priority: SharedKey (account key) → SAS token → anonymous.

use super::{auth, AzureConfig};
use crate::{Value, ValueRef};
use niao_errors::codes;
use niao_ast::Span;
use crate::error_value;
use std::collections::HashMap;

const BLOB_VERSION: &str = "2020-08-04";

fn blob_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2811_NAZURE_ERROR, "nazure_error", msg.into(), span)
}

fn auth_error(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2813_NAZURE_AUTH, "nazure_error", msg.into(), span)
}

/// Build `https://{account}.blob.core.windows.net/{container}/{blob_path}`
/// and optionally append a query string.
fn blob_url(account: &str, container: &str, blob_path: &str, query: Option<&str>) -> String {
    let base = format!(
        "https://{}.blob.core.windows.net/{}/{}",
        account, container, blob_path
    );
    match query {
        Some(q) if !q.is_empty() => format!("{base}?{q}"),
        _ => base,
    }
}

/// Returns `(auth_header_value, query_suffix)`.
/// `query_suffix` is non-empty only when using SAS (so it gets appended to the URL).
fn make_blob_auth(
    cfg: &AzureConfig,
    method: &str,
    content_length: &str,
    content_type: &str,
    date: &str,
    ms_headers: &[(String, String)],
    canon_resource: &str,
) -> Result<(Option<String>, String), String> {
    if let Some(key) = &cfg.key {
        let auth = auth::shared_key_blob(
            &cfg.account,
            key,
            method,
            content_length,
            content_type,
            date,
            ms_headers,
            canon_resource,
        );
        Ok((Some(auth), String::new()))
    } else if let Some(sas) = &cfg.sas {
        // SAS token goes in the query string; no Authorization header.
        Ok((None, sas.clone()))
    } else if let (Some(tenant), Some(cid), Some(csec)) =
        (&cfg.tenant, &cfg.client_id, &cfg.client_secret)
    {
        let scope = format!(
            "https://{}.blob.core.windows.net/.default",
            cfg.account
        );
        let token =
            auth::fetch_bearer_token(tenant, cid, csec, &scope)?;
        Ok((Some(format!("Bearer {token}")), String::new()))
    } else {
        // Anonymous — no auth.
        Ok((None, String::new()))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Blob PUT
// ──────────────────────────────────────────────────────────────────────────────

pub fn blob_put(
    cfg: &AzureConfig,
    container: &str,
    blob: &str,
    body: Vec<u8>,
    content_type: &str,
    span: Span,
) -> ValueRef {
    let date = auth::rfc1123_now();
    let cl = body.len().to_string();
    let canon = format!("/{}/{}/{}", cfg.account, container, blob);
    let mut ms_hdrs = vec![
        ("x-ms-blob-type".to_string(), "BlockBlob".to_string()),
        ("x-ms-date".to_string(), date.clone()),
        ("x-ms-version".to_string(), BLOB_VERSION.to_string()),
    ];
    ms_hdrs.sort_by(|a, b| a.0.cmp(&b.0));

    let (auth_hdr, sas_qs) = match make_blob_auth(
        cfg, "PUT", &cl, content_type, &date, &ms_hdrs, &canon,
    ) {
        Ok(v) => v,
        Err(e) => return auth_error(span, e),
    };

    let url = blob_url(&cfg.account, container, blob, if sas_qs.is_empty() { None } else { Some(&sas_qs) });
    let mut req = niao_http::put(&url)
        .set("x-ms-blob-type", "BlockBlob")
        .set("x-ms-date", &date)
        .set("x-ms-version", BLOB_VERSION)
        .set("Content-Type", content_type)
        .set("Content-Length", &cl);
    if let Some(h) = auth_hdr {
        req = req.set("Authorization", h);
    }

    match req.send_bytes(&body) {
        Err(e) => blob_error(span, format!("nazure blob_put: {e}")),
        Ok(resp) => {
            let status = resp.status as i64;
            let etag = resp.header("etag").unwrap_or("").to_string();
            if !(200..300).contains(&(status as u16)) {
                let body_str = String::from_utf8_lossy(&resp.body).into_owned();
                return blob_error(span, format!("blob_put HTTP {status}: {body_str}"));
            }
            let mut map = HashMap::new();
            map.insert("status".into(), Value::Int(status).ref_cell());
            map.insert("etag".into(), Value::String(etag).ref_cell());
            Value::Object(map).ref_cell()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Blob GET
// ──────────────────────────────────────────────────────────────────────────────

pub fn blob_get(
    cfg: &AzureConfig,
    container: &str,
    blob: &str,
    span: Span,
) -> ValueRef {
    let date = auth::rfc1123_now();
    let canon = format!("/{}/{}/{}", cfg.account, container, blob);
    let mut ms_hdrs = vec![
        ("x-ms-date".to_string(), date.clone()),
        ("x-ms-version".to_string(), BLOB_VERSION.to_string()),
    ];
    ms_hdrs.sort_by(|a, b| a.0.cmp(&b.0));

    let (auth_hdr, sas_qs) = match make_blob_auth(
        cfg, "GET", "", "", &date, &ms_hdrs, &canon,
    ) {
        Ok(v) => v,
        Err(e) => return auth_error(span, e),
    };

    let url = blob_url(&cfg.account, container, blob, if sas_qs.is_empty() { None } else { Some(&sas_qs) });
    let mut req = niao_http::get(&url)
        .set("x-ms-date", &date)
        .set("x-ms-version", BLOB_VERSION);
    if let Some(h) = auth_hdr {
        req = req.set("Authorization", h);
    }

    match req.send() {
        Err(e) => blob_error(span, format!("nazure blob_get: {e}")),
        Ok(resp) => {
            let status = resp.status as i64;
            let body = String::from_utf8_lossy(&resp.body).into_owned();
            if !(200..300).contains(&(status as u16)) {
                return blob_error(span, format!("blob_get HTTP {status}: {body}"));
            }
            let mut map = HashMap::new();
            map.insert("status".into(), Value::Int(status).ref_cell());
            map.insert("body".into(), Value::String(body).ref_cell());
            Value::Object(map).ref_cell()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Blob DELETE
// ──────────────────────────────────────────────────────────────────────────────

pub fn blob_delete(
    cfg: &AzureConfig,
    container: &str,
    blob: &str,
    span: Span,
) -> ValueRef {
    let date = auth::rfc1123_now();
    let canon = format!("/{}/{}/{}", cfg.account, container, blob);
    let mut ms_hdrs = vec![
        ("x-ms-date".to_string(), date.clone()),
        ("x-ms-version".to_string(), BLOB_VERSION.to_string()),
    ];
    ms_hdrs.sort_by(|a, b| a.0.cmp(&b.0));

    let (auth_hdr, sas_qs) = match make_blob_auth(
        cfg, "DELETE", "", "", &date, &ms_hdrs, &canon,
    ) {
        Ok(v) => v,
        Err(e) => return auth_error(span, e),
    };

    let url = blob_url(&cfg.account, container, blob, if sas_qs.is_empty() { None } else { Some(&sas_qs) });
    let mut req = niao_http::delete(&url)
        .set("x-ms-date", &date)
        .set("x-ms-version", BLOB_VERSION);
    if let Some(h) = auth_hdr {
        req = req.set("Authorization", h);
    }

    match req.send() {
        Err(e) => blob_error(span, format!("nazure blob_delete: {e}")),
        Ok(resp) => {
            let status = resp.status;
            if status == 202 || status == 204 {
                Value::Bool(true).ref_cell()
            } else {
                let body = String::from_utf8_lossy(&resp.body).into_owned();
                blob_error(span, format!("blob_delete HTTP {status}: {body}"))
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Blob LIST
// ──────────────────────────────────────────────────────────────────────────────

pub fn blob_list(
    cfg: &AzureConfig,
    container: &str,
    prefix: Option<&str>,
    span: Span,
) -> ValueRef {
    let date = auth::rfc1123_now();

    // Canonicalized resource for list: /{account}/{container}\ncomp:list[\nprefix:{p}]\nrestype:container
    let mut canon_parts = vec!["comp:list".to_string()];
    if let Some(p) = prefix {
        if !p.is_empty() {
            canon_parts.push(format!("prefix:{p}"));
        }
    }
    canon_parts.push("restype:container".to_string());
    canon_parts.sort_unstable();
    let canon = format!(
        "/{}/{}\n{}",
        cfg.account,
        container,
        canon_parts.join("\n")
    );

    let mut ms_hdrs = vec![
        ("x-ms-date".to_string(), date.clone()),
        ("x-ms-version".to_string(), BLOB_VERSION.to_string()),
    ];
    ms_hdrs.sort_by(|a, b| a.0.cmp(&b.0));

    let (auth_hdr, sas_qs) = match make_blob_auth(
        cfg, "GET", "", "", &date, &ms_hdrs, &canon,
    ) {
        Ok(v) => v,
        Err(e) => return auth_error(span, e),
    };

    // Build query string
    let mut qparams = vec![
        "restype=container".to_string(),
        "comp=list".to_string(),
    ];
    if let Some(p) = prefix {
        if !p.is_empty() {
            qparams.push(format!("prefix={}", niao_http::percent_encode(p.as_bytes())));
        }
    }
    let mut qs_parts = qparams.join("&");
    if !sas_qs.is_empty() {
        qs_parts.push('&');
        qs_parts.push_str(&sas_qs);
    }
    let url = blob_url(&cfg.account, container, "", Some(&qs_parts));

    let mut req = niao_http::get(&url)
        .set("x-ms-date", &date)
        .set("x-ms-version", BLOB_VERSION);
    if let Some(h) = auth_hdr {
        req = req.set("Authorization", h);
    }

    match req.send() {
        Err(e) => blob_error(span, format!("nazure blob_list: {e}")),
        Ok(resp) => {
            let status = resp.status;
            let body = String::from_utf8_lossy(&resp.body).into_owned();
            if !(200..300).contains(&status) {
                return blob_error(span, format!("blob_list HTTP {status}: {body}"));
            }
            let names = extract_xml_tags(&body, "Name");
            Value::Array(
                names
                    .into_iter()
                    .map(|n| Value::String(n).ref_cell())
                    .collect(),
            )
            .ref_cell()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// XML helper (no external crate — simple tag extraction)
// ──────────────────────────────────────────────────────────────────────────────

/// Extract text content from all occurrences of `<tag>...</tag>` in `xml`.
pub(super) fn extract_xml_tags(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        rest = &rest[start + open.len()..];
        if let Some(end) = rest.find(&close) {
            out.push(rest[..end].to_string());
            rest = &rest[end + close.len()..];
        } else {
            break;
        }
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_xml_tags_basic() {
        let xml = r#"<Blobs><Blob><Name>foo.txt</Name></Blob><Blob><Name>bar/baz.json</Name></Blob></Blobs>"#;
        assert_eq!(extract_xml_tags(xml, "Name"), vec!["foo.txt", "bar/baz.json"]);
    }

    #[test]
    fn extract_xml_tags_empty() {
        assert_eq!(extract_xml_tags("<Blobs></Blobs>", "Name"), Vec::<String>::new());
    }

    #[test]
    fn extract_xml_tags_nested() {
        let xml = "<root><item><Name>alpha</Name></item><item><Name>beta</Name></item></root>";
        assert_eq!(extract_xml_tags(xml, "Name"), vec!["alpha", "beta"]);
    }
}
