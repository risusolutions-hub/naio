//! naws S3 operations: put, get, delete, list.

use super::{
    aws_error, get_config, ok_bool, ok_string, ok_value, string_opt, AwsResult,
};
use crate::{Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;

use super::sigv4::{now_amz, sign, uri_encode, SignInput};

/// `naws.s3_put(config_id, bucket, key, body, content_type?) → {etag, status}`
pub fn s3_put(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() < 4 || args.len() > 5 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_s3_put() expects 4-5 arguments: config, bucket, key, body, content_type?",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_s3_put", span)?;
    let cfg = get_config(config_id, span)?;
    let bucket = super::str_arg(args, 1, "naws_s3_put", span)?;
    let key = super::str_arg(args, 2, "naws_s3_put", span)?;
    let body = super::bytes_arg(args, 3, "naws_s3_put", span)?;
    let content_type = string_opt(args, 4).unwrap_or_else(|| "application/octet-stream".into());

    let host = s3_host(&cfg.region, &bucket);
    let path = format!("/{}", uri_encode(&key, false));
    let (amz_dt, amz_d) = now_amz();

    let extra = [("content-type", content_type.as_str())];
    let inp = SignInput {
        method: "PUT",
        host: &host,
        path: &path,
        query: "",
        region: &cfg.region,
        service: "s3",
        access_key: &cfg.access_key,
        secret_key: &cfg.secret_key,
        session_token: cfg.session_token.as_deref(),
        body: &body,
        amz_datetime: &amz_dt,
        amz_date: &amz_d,
        extra_headers: &extra,
    };
    let signed = sign(&inp);

    let url = format!("https://{}{}", host, path);
    let mut builder = niao_http::put(&url);
    for (k, v) in &signed.headers {
        builder = builder.set(k.clone(), v.clone());
    }
    builder = builder.set("Content-Type", &content_type);

    match builder.send_bytes(&body) {
        Ok(resp) => {
            let status = resp.status as i64;
            if status >= 400 {
                let body_str = String::from_utf8_lossy(&resp.body).into_owned();
                return Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", body_str, span));
            }
            let etag = resp
                .header("etag")
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default();
            let mut map = HashMap::new();
            map.insert("etag".into(), ok_string(etag));
            map.insert("status".into(), ok_value(Value::Int(status)));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", e.to_string(), span)),
    }
}

/// `naws.s3_get(config_id, bucket, key) → {body, status, headers{}}`
pub fn s3_get(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_s3_get() expects 3 arguments: config, bucket, key",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_s3_get", span)?;
    let cfg = get_config(config_id, span)?;
    let bucket = super::str_arg(args, 1, "naws_s3_get", span)?;
    let key = super::str_arg(args, 2, "naws_s3_get", span)?;

    let host = s3_host(&cfg.region, &bucket);
    let path = format!("/{}", uri_encode(&key, false));
    let (amz_dt, amz_d) = now_amz();

    let inp = SignInput {
        method: "GET",
        host: &host,
        path: &path,
        query: "",
        region: &cfg.region,
        service: "s3",
        access_key: &cfg.access_key,
        secret_key: &cfg.secret_key,
        session_token: cfg.session_token.as_deref(),
        body: b"",
        amz_datetime: &amz_dt,
        amz_date: &amz_d,
        extra_headers: &[],
    };
    let signed = sign(&inp);

    let url = format!("https://{}{}", host, path);
    let mut builder = niao_http::get(&url);
    for (k, v) in &signed.headers {
        builder = builder.set(k.clone(), v.clone());
    }

    match builder.send() {
        Ok(resp) => {
            let status = resp.status as i64;
            if status >= 400 {
                let body_str = String::from_utf8_lossy(&resp.body).into_owned();
                return Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", body_str, span));
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
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", e.to_string(), span)),
    }
}

/// `naws.s3_delete(config_id, bucket, key) → true`
pub fn s3_delete(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() != 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_s3_delete() expects 3 arguments: config, bucket, key",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_s3_delete", span)?;
    let cfg = get_config(config_id, span)?;
    let bucket = super::str_arg(args, 1, "naws_s3_delete", span)?;
    let key = super::str_arg(args, 2, "naws_s3_delete", span)?;

    let host = s3_host(&cfg.region, &bucket);
    let path = format!("/{}", uri_encode(&key, false));
    let (amz_dt, amz_d) = now_amz();

    let inp = SignInput {
        method: "DELETE",
        host: &host,
        path: &path,
        query: "",
        region: &cfg.region,
        service: "s3",
        access_key: &cfg.access_key,
        secret_key: &cfg.secret_key,
        session_token: cfg.session_token.as_deref(),
        body: b"",
        amz_datetime: &amz_dt,
        amz_date: &amz_d,
        extra_headers: &[],
    };
    let signed = sign(&inp);

    let url = format!("https://{}{}", host, path);
    let mut builder = niao_http::delete(&url);
    for (k, v) in &signed.headers {
        builder = builder.set(k.clone(), v.clone());
    }

    match builder.send() {
        Ok(resp) => {
            let status = resp.status as i64;
            if status >= 400 {
                let body_str = String::from_utf8_lossy(&resp.body).into_owned();
                return Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", body_str, span));
            }
            Ok(ok_bool(true))
        }
        Err(e) => Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", e.to_string(), span)),
    }
}

/// `naws.s3_list(config_id, bucket, prefix?) → keys[]`
pub fn s3_list(args: &[ValueRef], span: Span) -> AwsResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(crate::RuntimeError::at(
            span,
            codes::E2800_NAWS_ARITY,
            "naws_s3_list() expects 2-3 arguments: config, bucket, prefix?",
        ));
    }
    let config_id = super::int_arg(args, 0, "naws_s3_list", span)?;
    let cfg = get_config(config_id, span)?;
    let bucket = super::str_arg(args, 1, "naws_s3_list", span)?;
    let prefix = string_opt(args, 2).unwrap_or_default();

    let host = s3_host(&cfg.region, &bucket);
    let path = "/";
    let query = if prefix.is_empty() {
        "list-type=2".to_string()
    } else {
        format!("list-type=2&prefix={}", uri_encode(&prefix, true))
    };
    let (amz_dt, amz_d) = now_amz();

    let inp = SignInput {
        method: "GET",
        host: &host,
        path,
        query: &query,
        region: &cfg.region,
        service: "s3",
        access_key: &cfg.access_key,
        secret_key: &cfg.secret_key,
        session_token: cfg.session_token.as_deref(),
        body: b"",
        amz_datetime: &amz_dt,
        amz_date: &amz_d,
        extra_headers: &[],
    };
    let signed = sign(&inp);

    let url = format!("https://{}/?{}", host, query);
    let mut builder = niao_http::get(&url);
    for (k, v) in &signed.headers {
        builder = builder.set(k.clone(), v.clone());
    }

    match builder.send() {
        Ok(resp) => {
            let status = resp.status as i64;
            let body_str = String::from_utf8_lossy(&resp.body).into_owned();
            if status >= 400 {
                return Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", body_str, span));
            }
            let keys = parse_s3_list_keys(&body_str);
            Ok(Value::Array(keys.into_iter().map(|k| ok_string(k)).collect()).ref_cell())
        }
        Err(e) => Ok(aws_error(codes::E2801_NAWS_ERROR, "naws_s3_error", e.to_string(), span)),
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn s3_host(region: &str, bucket: &str) -> String {
    format!("{bucket}.s3.{region}.amazonaws.com")
}

/// Minimal S3 ListObjects XML parser — extracts `<Key>` elements.
fn parse_s3_list_keys(xml: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<Key>") {
        rest = &rest[start + 5..];
        if let Some(end) = rest.find("</Key>") {
            keys.push(rest[..end].to_string());
            rest = &rest[end + 6..];
        } else {
            break;
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_list_keys_basic() {
        let xml = r#"<?xml version="1.0"?>
<ListBucketResult>
  <Key>folder/file1.txt</Key>
  <Key>folder/file2.txt</Key>
</ListBucketResult>"#;
        let keys = parse_s3_list_keys(xml);
        assert_eq!(keys, ["folder/file1.txt", "folder/file2.txt"]);
    }

    #[test]
    fn parse_list_keys_empty() {
        let xml = "<ListBucketResult></ListBucketResult>";
        assert!(parse_s3_list_keys(xml).is_empty());
    }
}
