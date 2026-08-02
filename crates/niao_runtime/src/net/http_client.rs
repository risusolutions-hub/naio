//! HTTP/HTTPS client via `niao_http`.

use super::{net_error, ok_string, parse_http_opts, string_arg, HttpOpts, NetResult};
use niao_ast::Span;
use niao_errors::codes;
use niao_http::{delete, get, head, post, put, request, Method, RequestBuilder};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;

fn apply_opts(mut builder: RequestBuilder, opts: &HttpOpts) -> RequestBuilder {
    if let Some(ms) = opts.timeout_ms {
        builder = builder.timeout(Duration::from_millis(ms));
    }
    if let Some(ua) = &opts.user_agent {
        builder = builder.set("User-Agent", ua);
    }
    if let Some((user, pass)) = &opts.auth {
        let enc = niao_codec::base64::encode_standard(format!("{user}:{pass}").as_bytes());
        builder = builder.set("Authorization", format!("Basic {enc}"));
    }
    for (k, v) in &opts.headers {
        builder = builder.set(k.clone(), v.clone());
    }
    builder
}

pub fn http_request(
    method: &str,
    url: &str,
    opts: HttpOpts,
    span: Span,
) -> Result<crate::ValueRef, crate::ValueRef> {
    let builder = match method.to_uppercase().as_str() {
        "GET" => {
            if opts.body.is_some() || opts.body_bytes.is_some() {
                return Err(net_error(
                    span,
                    codes::E1404_NET_HTTP,
                    "net_http_error",
                    "GET cannot include a body",
                ));
            }
            apply_opts(get(url), &opts)
        }
        "HEAD" => apply_opts(head(url), &opts),
        "POST" => apply_opts(post(url), &opts),
        "PUT" => apply_opts(put(url), &opts),
        "DELETE" => apply_opts(delete(url), &opts),
        "PATCH" => apply_opts(request(Method::Patch, url), &opts),
        other => {
            let Some(m) = Method::parse(other) else {
                return Err(net_error(
                    span,
                    codes::E1404_NET_HTTP,
                    "net_http_error",
                    format!("unsupported HTTP method: {other}"),
                ));
            };
            apply_opts(request(m, url), &opts)
        }
    };

    let result = send_body(builder, &opts);
    match result {
        Ok(resp) => Ok(response_to_value(resp, url)),
        Err(e) => Err(net_error(
            span,
            codes::E1401_NET_ERROR,
            "net_error",
            e.to_string(),
        )),
    }
}

fn send_body(
    builder: RequestBuilder,
    opts: &HttpOpts,
) -> Result<niao_http::Response, niao_http::Error> {
    if let Some(body) = &opts.body {
        builder.send_string(body)
    } else if let Some(bytes) = &opts.body_bytes {
        let data: Vec<u8> = bytes.iter().map(|&b| b as u8).collect();
        builder.send_bytes(&data)
    } else {
        builder.send()
    }
}

fn response_to_value(resp: niao_http::Response, url: &str) -> crate::ValueRef {
    let status = resp.status as i64;
    let final_url = resp.url.clone();
    let mut headers = HashMap::new();
    for name in resp.headers_names() {
        if let Some(v) = resp.header(&name) {
            headers.insert(name.to_lowercase(), ok_string(v.to_string()));
        }
    }
    let body_bytes = resp.body;
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    let ok = (200..300).contains(&(status as u16));
    let mut map = HashMap::new();
    map.insert("status".into(), crate::Value::Int(status).ref_cell());
    map.insert("ok".into(), crate::Value::Bool(ok).ref_cell());
    map.insert(
        "url".into(),
        ok_string(if final_url.is_empty() {
            url.into()
        } else {
            final_url
        }),
    );
    map.insert("body".into(), ok_string(body));
    map.insert(
        "body_bytes".into(),
        crate::Value::IntArray(body_bytes.into_iter().map(|b| b as i64).collect()).ref_cell(),
    );
    map.insert("headers".into(), crate::Value::Object(headers).ref_cell());
    crate::Value::Object(map).ref_cell()
}

fn parse_opts(args: &[crate::ValueRef], start: usize, span: Span) -> HttpOpts {
    if args.len() <= start {
        return HttpOpts::default();
    }
    parse_http_opts(args[start].clone(), span)
}

pub fn net_http_get(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 1, 2, "net_http_get", span)?;
    let url = string_arg(args, 0, "net_http_get", span)?;
    let opts = parse_opts(args, 1, span);
    match http_request("GET", &url, opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

pub fn net_http_post(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 2, 3, "net_http_post", span)?;
    let url = string_arg(args, 0, "net_http_post", span)?;
    let body = string_arg(args, 1, "net_http_post", span)?;
    let mut opts = parse_opts(args, 2, span);
    opts.body = Some(body);
    match http_request("POST", &url, opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

pub fn net_http_put(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 2, 3, "net_http_put", span)?;
    let url = string_arg(args, 0, "net_http_put", span)?;
    let body = string_arg(args, 1, "net_http_put", span)?;
    let mut opts = parse_opts(args, 2, span);
    opts.body = Some(body);
    match http_request("PUT", &url, opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

pub fn net_http_delete(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 1, 2, "net_http_delete", span)?;
    let url = string_arg(args, 0, "net_http_delete", span)?;
    let opts = parse_opts(args, 1, span);
    match http_request("DELETE", &url, opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

pub fn net_http_patch(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 2, 3, "net_http_patch", span)?;
    let url = string_arg(args, 0, "net_http_patch", span)?;
    let body = string_arg(args, 1, "net_http_patch", span)?;
    let mut opts = parse_opts(args, 2, span);
    opts.body = Some(body);
    match http_request("PATCH", &url, opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

pub fn net_http_head(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 1, 2, "net_http_head", span)?;
    let url = string_arg(args, 0, "net_http_head", span)?;
    let opts = parse_opts(args, 1, span);
    match http_request("HEAD", &url, opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

pub fn net_http_request(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 2, 3, "net_http_request", span)?;
    let method = string_arg(args, 0, "net_http_request", span)?;
    let url = string_arg(args, 1, "net_http_request", span)?;
    let opts = parse_opts(args, 2, span);
    match http_request(&method, &url, opts, span) {
        Ok(v) => Ok(v),
        Err(e) => Ok(e),
    }
}

pub fn net_http_download(args: &[crate::ValueRef], span: Span) -> NetResult {
    super::arity_range(args, 2, 3, "net_http_download", span)?;
    let url = string_arg(args, 0, "net_http_download", span)?;
    let path = string_arg(args, 1, "net_http_download", span)?;
    let opts = parse_opts(args, 2, span);
    match http_request("GET", &url, opts, span) {
        Ok(resp) => {
            let bytes = super::response_body_bytes(&resp.borrow());
            match fs::write(&path, bytes) {
                Ok(()) => Ok(resp),
                Err(e) => Ok(net_error(
                    span,
                    codes::E1401_NET_ERROR,
                    "net_error",
                    e.to_string(),
                )),
            }
        }
        Err(e) => Ok(e),
    }
}

pub fn response_to_async(resp: crate::ValueRef) -> crate::async_tasks::AsyncValue {
    super::value_to_async_response(&resp.borrow())
}
