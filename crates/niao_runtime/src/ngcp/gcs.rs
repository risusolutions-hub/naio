//! GCS JSON API — put / get / delete / list.

use super::{
    bearer_auth, gcp_error, ok_string, ok_value, with_config_mut, GcpResult,
};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

fn gcs_error(span: Span, msg: impl Into<String>) -> ValueRef {
    gcp_error(codes::E4541_NGCP_ERROR, "ngcp_gcs_error", msg, span)
}

/// `ngcp.gcs_put(cfg, bucket, object, body, content_type?) → {etag, status}`
///
/// // >>> ngcp.gcs_put != nil
/// // => true
pub fn gcs_put(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() < 4 || args.len() > 5 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_gcs_put() expects 4-5 arguments: config, bucket, object, body, content_type?",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_gcs_put", span)?;
    let bucket = super::str_arg(args, 1, "ngcp_gcs_put", span)?;
    let object = super::str_arg(args, 2, "ngcp_gcs_put", span)?;
    let body = super::bytes_arg(args, 3, "ngcp_gcs_put", span)?;
    let content_type = super::string_opt(args, 4)
        .unwrap_or_else(|| "application/octet-stream".into());

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return gcs_error(span, e),
        };
        let name = crate::ngcp::auth::uri_encode_path(&object);
        let url = format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            crate::ngcp::auth::uri_encode_path(&bucket),
            name
        );
        match niao_http::post(&url)
            .set("Authorization", format!("Bearer {token}"))
            .set("Content-Type", &content_type)
            .send_bytes(&body)
        {
            Ok(resp) => {
                let status = resp.status as i64;
                if status >= 400 {
                    return gcs_error(span, String::from_utf8_lossy(&resp.body));
                }
                let text = String::from_utf8_lossy(&resp.body);
                let etag = extract_quoted(&text, "etag").unwrap_or_default();
                let mut map = HashMap::new();
                map.insert("etag".into(), ok_string(etag));
                map.insert("status".into(), ok_value(Value::Int(status)));
                Value::Object(map).ref_cell()
            }
            Err(e) => gcs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `ngcp.gcs_get(cfg, bucket, object) → {body, status, headers{}}`
///
/// // >>> ngcp.gcs_get != nil
/// // => true
pub fn gcs_get(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_gcs_get() expects 3 arguments: config, bucket, object",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_gcs_get", span)?;
    let bucket = super::str_arg(args, 1, "ngcp_gcs_get", span)?;
    let object = super::str_arg(args, 2, "ngcp_gcs_get", span)?;

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return gcs_error(span, e),
        };
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
            crate::ngcp::auth::uri_encode_path(&bucket),
            crate::ngcp::auth::uri_encode_path(&object)
        );
        match niao_http::get(&url)
            .set("Authorization", format!("Bearer {token}"))
            .send()
        {
            Ok(resp) => {
                let status = resp.status as i64;
                if status >= 400 {
                    return gcs_error(span, String::from_utf8_lossy(&resp.body));
                }
                let body_str = String::from_utf8_lossy(&resp.body).into_owned();
                let mut hdrs = HashMap::new();
                for name in resp.headers_names() {
                    if let Some(v) = resp.header(&name) {
                        hdrs.insert(name.to_lowercase(), ok_string(v.to_string()));
                    }
                }
                let mut map = HashMap::new();
                map.insert("body".into(), ok_string(body_str));
                map.insert("status".into(), ok_value(Value::Int(status)));
                map.insert("headers".into(), ok_value(Value::Object(hdrs)));
                Value::Object(map).ref_cell()
            }
            Err(e) => gcs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `ngcp.gcs_delete(cfg, bucket, object) → true`
///
/// // >>> ngcp.gcs_delete != nil
/// // => true
pub fn gcs_delete(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_gcs_delete() expects 3 arguments: config, bucket, object",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_gcs_delete", span)?;
    let bucket = super::str_arg(args, 1, "ngcp_gcs_delete", span)?;
    let object = super::str_arg(args, 2, "ngcp_gcs_delete", span)?;

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return gcs_error(span, e),
        };
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}",
            crate::ngcp::auth::uri_encode_path(&bucket),
            crate::ngcp::auth::uri_encode_path(&object)
        );
        match niao_http::delete(&url)
            .set("Authorization", format!("Bearer {token}"))
            .send()
        {
            Ok(resp) => {
                let status = resp.status as i64;
                if status >= 400 && status != 404 {
                    return gcs_error(span, String::from_utf8_lossy(&resp.body));
                }
                Value::Bool(true).ref_cell()
            }
            Err(e) => gcs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

/// `ngcp.gcs_list(cfg, bucket, prefix?) → names[]`
///
/// // >>> ngcp.gcs_list != nil
/// // => true
pub fn gcs_list(args: &[ValueRef], span: Span) -> GcpResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E4540_NGCP_ARITY,
            "ngcp_gcs_list() expects 2-3 arguments: config, bucket, prefix?",
        ));
    }
    let id = super::int_arg(args, 0, "ngcp_gcs_list", span)?;
    let bucket = super::str_arg(args, 1, "ngcp_gcs_list", span)?;
    let prefix = super::string_opt(args, 2);

    match with_config_mut(id, span, |cfg| {
        let token = match bearer_auth(cfg) {
            Ok(t) => t,
            Err(e) => return gcs_error(span, e),
        };
        let mut url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o?fields=items(name)&maxResults=1000",
            crate::ngcp::auth::uri_encode_path(&bucket)
        );
        if let Some(p) = &prefix {
            url.push_str("&prefix=");
            url.push_str(&crate::ngcp::auth::uri_encode_path(p));
        }
        match niao_http::get(&url)
            .set("Authorization", format!("Bearer {token}"))
            .send()
        {
            Ok(resp) => {
                let status = resp.status as i64;
                if status >= 400 {
                    return gcs_error(span, String::from_utf8_lossy(&resp.body));
                }
                let text = String::from_utf8_lossy(&resp.body);
                let names = extract_name_array(&text);
                let arr: Vec<ValueRef> = names.into_iter().map(ok_string).collect();
                Value::Array(arr).ref_cell()
            }
            Err(e) => gcs_error(span, e.to_string()),
        }
    }) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

fn extract_quoted(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)?;
    let after = json[start + needle.len()..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

/// Pull `"name":"..."` entries from a GCS list JSON response.
fn extract_name_array(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(idx) = rest.find("\"name\"") {
        let after = rest[idx + 6..].trim_start();
        let after = match after.strip_prefix(':') {
            Some(a) => a.trim_start(),
            None => {
                rest = &rest[idx + 6..];
                continue;
            }
        };
        let after = match after.strip_prefix('"') {
            Some(a) => a,
            None => {
                rest = after;
                continue;
            }
        };
        if let Some(end) = after.find('"') {
            out.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_name_array_multiple() {
        let j = r#"{"items":[{"name":"a.txt"},{"name":"dir/b.json"}]}"#;
        assert_eq!(extract_name_array(j), vec!["a.txt", "dir/b.json"]);
    }

    #[test]
    fn extract_name_array_empty() {
        assert!(extract_name_array("{}").is_empty());
    }
}
