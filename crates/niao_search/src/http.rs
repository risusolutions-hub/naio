//! Thin HTTP boundary over `niao_http` (keeps search logic portable).

use crate::client::{
    auth_headers, meili_auth_headers, typesense_auth_headers, Auth, Client, Engine,
};
use crate::error::{SearchError, SearchResult};
use niao_http::{delete, get, post, put, request, Method};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub ok: bool,
    pub url: String,
    pub body: String,
    pub elapsed_ms: u64,
}

impl HttpResponse {
    pub fn from_parts(status: u16, url: String, body: String, elapsed_ms: u64) -> Self {
        Self {
            ok: (200..300).contains(&status),
            status,
            url,
            body,
            elapsed_ms,
        }
    }
}

pub struct RequestOpts {
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub headers: Vec<(String, String)>,
    pub params: Vec<(String, String)>,
}

impl Default for RequestOpts {
    fn default() -> Self {
        Self {
            body: None,
            content_type: Some("application/json".into()),
            headers: Vec::new(),
            params: Vec::new(),
        }
    }
}

fn apply_headers(
    mut rb: niao_http::RequestBuilder,
    headers: &[(String, String)],
    timeout_ms: u64,
) -> niao_http::RequestBuilder {
    for (k, v) in headers {
        rb = rb.header(k, v);
    }
    if timeout_ms > 0 {
        rb = rb.timeout(Duration::from_millis(timeout_ms));
    }
    rb
}

pub fn execute(
    client: &Client,
    method: &str,
    path: &str,
    opts: &RequestOpts,
) -> SearchResult<HttpResponse> {
    let url = crate::query::prepare_url(&client.base_url, Some(path), &opts.params)?;
    let mut headers = match client.engine {
        Engine::Meilisearch => match &client.auth {
            Auth::Bearer(k) | Auth::ApiKey(k) => meili_auth_headers(k),
            Auth::Basic { .. } => auth_headers(&client.auth),
            Auth::None => Vec::new(),
        },
        Engine::Typesense => match &client.auth {
            Auth::ApiKey(k) | Auth::Bearer(k) => typesense_auth_headers(k),
            other => auth_headers(other),
        },
        Engine::Elasticsearch | Engine::OpenSearch => auth_headers(&client.auth),
    };
    for (k, v) in &client.default_headers {
        headers.push((k.clone(), v.clone()));
    }
    for (k, v) in &opts.headers {
        headers.push((k.clone(), v.clone()));
    }
    if let Some(ct) = &opts.content_type {
        headers.push(("content-type".into(), ct.clone()));
    }
    headers.push(("accept".into(), "application/json".into()));

    let t0 = Instant::now();
    let method_u = method.to_ascii_uppercase();
    let resp = match method_u.as_str() {
        "GET" => {
            let rb = apply_headers(get(&url), &headers, client.timeout_ms);
            rb.send()
        }
        "HEAD" => {
            let rb = apply_headers(niao_http::head(&url), &headers, client.timeout_ms);
            rb.send()
        }
        "DELETE" => {
            let rb = apply_headers(delete(&url), &headers, client.timeout_ms);
            if let Some(body) = &opts.body {
                rb.send_string(body)
            } else {
                rb.send()
            }
        }
        "PUT" => {
            let rb = apply_headers(put(&url), &headers, client.timeout_ms);
            rb.send_string(opts.body.as_deref().unwrap_or(""))
        }
        "POST" => {
            let rb = apply_headers(post(&url), &headers, client.timeout_ms);
            rb.send_string(opts.body.as_deref().unwrap_or(""))
        }
        other => {
            let m = Method::parse(other).ok_or_else(|| {
                SearchError::Protocol(format!("unsupported HTTP method: {other}"))
            })?;
            let rb = apply_headers(request(m, &url), &headers, client.timeout_ms);
            if let Some(body) = &opts.body {
                rb.send_string(body)
            } else {
                rb.send()
            }
        }
    }
    .map_err(|e| SearchError::Http(e.to_string()))?;

    let elapsed_ms = t0.elapsed().as_millis() as u64;
    let body = String::from_utf8_lossy(&resp.body).into_owned();
    Ok(HttpResponse::from_parts(
        resp.status,
        resp.url,
        body,
        elapsed_ms,
    ))
}
