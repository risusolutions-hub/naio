//! Request execution: URL prep, retries, cookies, proxy (HTTP), download.

use crate::auth::{basic_auth, bearer};
use crate::cookie::cookie_header_from_map;
use crate::error::{ReqError, ReqResult};
use crate::form::encode_form_map;
use crate::multipart::{build_multipart, MultipartPart};
use crate::response::{from_http, Response};
use crate::session::{RequestOpts, Session, DEFAULT_USER_AGENT};
use niao_http::{
    delete, form_urlencode, get, head, join as http_join, parse_url, post, put, request, Method,
};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

/// Join base URL + relative path (requests-style).
pub fn join_url(base: &str, path: &str) -> ReqResult<String> {
    if path.is_empty() {
        return Ok(base.to_string());
    }
    if path.contains("://") {
        return Ok(path.to_string());
    }
    if base.is_empty() {
        return Ok(path.to_string());
    }
    let base_url = parse_url(base).map_err(ReqError::Url)?;
    let joined = http_join(&base_url, path).map_err(ReqError::Url)?;
    Ok(joined.to_string_full())
}

/// Build URL with query parameters.
pub fn prepare_url(
    base: &str,
    path: Option<&str>,
    params: &[(String, String)],
) -> ReqResult<String> {
    let mut url = match path {
        Some(p) if !p.is_empty() => join_url(base, p)?,
        _ => base.to_string(),
    };
    if params.is_empty() {
        return Ok(url);
    }
    let qs = {
        let mut parts = Vec::with_capacity(params.len());
        for (k, v) in params {
            parts.push(format!(
                "{}={}",
                form_urlencode(k.as_bytes()),
                form_urlencode(v.as_bytes())
            ));
        }
        parts.join("&")
    };
    if url.contains('?') {
        if !url.ends_with('?') && !url.ends_with('&') {
            url.push('&');
        }
        url.push_str(&qs);
    } else {
        url.push('?');
        url.push_str(&qs);
    }
    Ok(url)
}

fn resolve_url(session: &Session, url: &str, opts: &RequestOpts) -> ReqResult<String> {
    let joined = if session.base_url.is_empty() || url.contains("://") {
        url.to_string()
    } else {
        join_url(&session.base_url, url)?
    };
    let mut pairs: Vec<(String, String)> = session
        .params
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (k, v) in &opts.params {
        pairs.push((k.clone(), v.clone()));
    }
    // stable order
    pairs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    // Dedup keeping last
    let mut map = BTreeMap::new();
    for (k, v) in pairs {
        map.insert(k, v);
    }
    let flat: Vec<(String, String)> = map.into_iter().collect();
    if flat.is_empty() {
        Ok(joined)
    } else {
        prepare_url(&joined, None, &flat)
    }
}

fn build_headers(
    session: &Session,
    opts: &RequestOpts,
    url: &str,
) -> ReqResult<Vec<(String, String)>> {
    let parsed = parse_url(url).map_err(ReqError::Url)?;
    let is_https = parsed.scheme == "https";
    let mut headers: Vec<(String, String)> = Vec::new();

    let ua = opts
        .user_agent
        .as_deref()
        .unwrap_or(session.user_agent.as_str());
    if !ua.is_empty() {
        headers.push(("User-Agent".into(), ua.into()));
    }

    for (k, v) in &session.headers {
        headers.push((k.clone(), v.clone()));
    }
    for (k, v) in &opts.headers {
        headers.push((k.clone(), v.clone()));
    }

    let mut cookie_map = session.cookies.to_map();
    for (k, v) in &opts.cookies {
        cookie_map.insert(k.clone(), v.clone());
    }
    if let Some(hdr) = session
        .cookies
        .header_for(&parsed.host, &parsed.path, is_https)
    {
        // Prefer jar Cookie header; merge extra cookies
        let mut merged = cookie_map;
        for part in hdr.split(';') {
            let part = part.trim();
            if let Some((n, v)) = part.split_once('=') {
                merged
                    .entry(n.trim().to_string())
                    .or_insert_with(|| v.trim().to_string());
            }
        }
        if !merged.is_empty() {
            headers.push(("Cookie".into(), cookie_header_from_map(&merged)));
        }
    } else if !cookie_map.is_empty() {
        headers.push(("Cookie".into(), cookie_header_from_map(&cookie_map)));
    }

    if let Some((u, p)) = opts.auth.as_ref().or(session.auth.as_ref()) {
        headers.push(("Authorization".into(), basic_auth(u, p)));
    } else if let Some(tok) = opts.bearer.as_ref().or(session.bearer.as_ref()) {
        headers.push(("Authorization".into(), bearer(tok)));
    }

    Ok(headers)
}

fn build_body(opts: &RequestOpts) -> ReqResult<(Vec<u8>, Option<String>)> {
    if !opts.files.is_empty() {
        let mut parts = opts.files.clone();
        if let Some(data) = &opts.data {
            // treat data string as extra field "data" only if no json
            if opts.json.is_none() {
                parts.push(MultipartPart::field("_", data.as_bytes()));
            }
        }
        let mp = build_multipart(&parts, None)?;
        let ct = mp.content_type();
        return Ok((mp.body, Some(ct)));
    }
    if let Some(ct) = &opts.content_type {
        if let Some(bytes) = &opts.body_bytes {
            return Ok((bytes.clone(), Some(ct.clone())));
        }
        if let Some(data) = &opts.data {
            return Ok((data.as_bytes().to_vec(), Some(ct.clone())));
        }
    }
    if let Some(json) = &opts.json {
        return Ok((json.as_bytes().to_vec(), Some("application/json".into())));
    }
    if let Some(bytes) = &opts.body_bytes {
        return Ok((bytes.clone(), opts.content_type.clone()));
    }
    if let Some(data) = &opts.data {
        // If looks like already encoded form (contains =), send as form
        let ct = opts
            .content_type
            .clone()
            .unwrap_or_else(|| "application/x-www-form-urlencoded".into());
        return Ok((data.as_bytes().to_vec(), Some(ct)));
    }
    Ok((Vec::new(), None))
}

fn should_retry(status: u16, statuses: &[u16]) -> bool {
    statuses.contains(&status)
}

/// Execute an HTTP request with session defaults, retries, and cookie updates.
pub fn execute(
    method: &str,
    url: &str,
    session: &mut Session,
    opts: &RequestOpts,
) -> ReqResult<Response> {
    let url = resolve_url(session, url, opts)?;
    let retries = opts.merge_retries(session);
    let backoff = opts.merge_backoff(session);
    let retry_statuses = opts
        .retry_statuses
        .clone()
        .unwrap_or_else(|| session.retry_statuses.clone());
    let allow_redirects = opts.merge_allow_redirects(session);
    let max_redirects = if allow_redirects {
        opts.merge_max_redirects(session).max(1)
    } else {
        0
    };

    let (body, content_type) = build_body(opts)?;
    let start = Instant::now();
    let mut attempt = 0u32;
    loop {
        let headers = build_headers(session, opts, &url)?;
        let result = if let Some(proxy) = opts.merge_proxy(session) {
            send_via_proxy(
                method,
                &url,
                proxy,
                &headers,
                &body,
                content_type.as_deref(),
                opts.merge_timeout(session),
                max_redirects,
            )
        } else {
            send_direct(
                method,
                &url,
                &headers,
                &body,
                content_type.as_deref(),
                session,
                opts,
                max_redirects,
            )
        };

        match result {
            Ok(mut resp) => {
                resp.elapsed_ms = start.elapsed().as_millis() as u64;
                // Store cookies
                if let Ok(parsed) = parse_url(&resp.url) {
                    session.cookies.store_from_response(
                        &resp.set_cookies,
                        &parsed.host,
                        &parsed.path,
                    );
                }
                if attempt < retries && should_retry(resp.status, &retry_statuses) {
                    attempt += 1;
                    sleep_backoff(backoff, attempt);
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                let retryable = matches!(
                    e,
                    ReqError::Timeout | ReqError::Io(_) | ReqError::Http(_) | ReqError::Proxy(_)
                );
                if attempt < retries && retryable {
                    attempt += 1;
                    sleep_backoff(backoff, attempt);
                    continue;
                }
                return Err(e);
            }
        }
    }
}

fn sleep_backoff(base_ms: u64, attempt: u32) {
    if base_ms == 0 {
        return;
    }
    let ms = base_ms.saturating_mul(1u64 << attempt.saturating_sub(1).min(8));
    thread::sleep(Duration::from_millis(ms));
}

fn send_direct(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    content_type: Option<&str>,
    session: &Session,
    opts: &RequestOpts,
    max_redirects: u8,
) -> ReqResult<Response> {
    let upper = method.to_ascii_uppercase();
    let m = Method::parse(&upper)
        .ok_or_else(|| ReqError::Config(format!("unsupported method: {method}")))?;
    let mut builder = match m {
        Method::Get => get(url),
        Method::Head => head(url),
        Method::Post => post(url),
        Method::Put => put(url),
        Method::Delete => delete(url),
        other => request(other, url),
    };
    let timeout = opts.merge_timeout(session);
    if timeout > 0 {
        builder = builder.timeout(Duration::from_millis(timeout));
    }
    for (k, v) in headers {
        builder = builder.set(k.clone(), v.clone());
    }
    if let Some(ct) = content_type {
        builder = builder.set("Content-Type", ct);
    }
    // niao_http ClientOptions.max_redirects — floor of 5 inside execute when >0;
    // we pass through; for allow_redirects=false we still get ≥5. Documented limitation.
    let _ = max_redirects;
    let resp = if body.is_empty() {
        builder.send()
    } else {
        builder.send_bytes(body)
    }?;
    Ok(from_http(resp, 0))
}

/// HTTP proxy (absolute-form request). HTTPS-via-CONNECT deferred.
fn send_via_proxy(
    method: &str,
    url: &str,
    proxy: &str,
    headers: &[(String, String)],
    body: &[u8],
    content_type: Option<&str>,
    timeout_ms: u64,
    _max_redirects: u8,
) -> ReqResult<Response> {
    let target = parse_url(url).map_err(ReqError::Url)?;
    if target.scheme == "https" {
        return Err(ReqError::Proxy(
            "HTTPS over HTTP proxy (CONNECT) not supported in nreq 0.1 — use direct TLS".into(),
        ));
    }
    let proxy_url = if proxy.contains("://") {
        proxy.to_string()
    } else {
        format!("http://{proxy}")
    };
    let proxy_parsed = parse_url(&proxy_url).map_err(|e| ReqError::Proxy(e))?;
    let addr = format!("{}:{}", proxy_parsed.host, proxy_parsed.port);
    let mut stream = TcpStream::connect(&addr).map_err(|e| ReqError::Proxy(e.to_string()))?;
    if timeout_ms > 0 {
        let d = Duration::from_millis(timeout_ms);
        let _ = stream.set_read_timeout(Some(d));
        let _ = stream.set_write_timeout(Some(d));
    }

    let mut req = format!(
        "{method} {url} HTTP/1.1\r\nHost: {}\r\n",
        target.authority()
    );
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("host") {
            continue;
        }
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(ct) = content_type {
        req.push_str(&format!("Content-Type: {ct}\r\n"));
    }
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    req.push_str("Proxy-Connection: close\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| ReqError::Io(e.to_string()))?;
    if !body.is_empty() {
        stream
            .write_all(body)
            .map_err(|e| ReqError::Io(e.to_string()))?;
    }

    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.len() > 16 * 1024 * 1024 {
                    return Err(ReqError::Io("response too large".into()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return Err(ReqError::Timeout),
            Err(e) => return Err(ReqError::Io(e.to_string())),
        }
    }
    let (head, off) =
        niao_http::parse_response(&buf).map_err(|e| ReqError::Http(format!("{e:?}")))?;
    let body_bytes = buf.get(off..).unwrap_or(&[]).to_vec();
    let mut headers_map = std::collections::HashMap::new();
    let mut set_cookies = Vec::new();
    for name in head.headers.names() {
        if let Some(v) = head.headers.get(&name) {
            let key = name.to_ascii_lowercase();
            if key == "set-cookie" {
                set_cookies.push(v.to_string());
            }
            headers_map.insert(key, v.to_string());
        }
    }
    Ok(Response {
        status: head.status,
        url: url.to_string(),
        headers: headers_map,
        body: body_bytes,
        elapsed_ms: 0,
        reason: crate::response::reason_phrase(head.status).into(),
        set_cookies,
    })
}

/// Download response body to a filesystem path (write after receive).
pub fn download(
    method: &str,
    url: &str,
    path: &str,
    session: &mut Session,
    opts: &RequestOpts,
) -> ReqResult<Response> {
    let resp = execute(method, url, session, opts)?;
    if let Err(e) = fs::write(path, &resp.body) {
        return Err(ReqError::Io(format!("write {path}: {e}")));
    }
    Ok(resp)
}

/// Encode form data helper for callers.
pub fn form_body(map: &BTreeMap<String, String>) -> String {
    encode_form_map(map)
}

pub fn default_user_agent() -> &'static str {
    DEFAULT_USER_AGENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_http::{OutgoingResponse, Server};
    use std::thread;

    #[test]
    fn prepare_url_params() {
        let u = prepare_url(
            "http://example.com/api",
            None,
            &[("q".into(), "a b".into()), ("x".into(), "1".into())],
        )
        .unwrap();
        assert!(u.contains("q=a+b") || u.contains("q=a%20b"));
        assert!(u.contains("x=1"));
    }

    #[test]
    fn local_get_json() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let url = format!("http://{addr}/hello");
        let handle = thread::spawn(move || {
            let req = server.recv().unwrap();
            assert_eq!(req.method(), "GET");
            req.respond(
                OutgoingResponse::from_string(r#"{"ok":true}"#)
                    .with_status(200)
                    .header("Content-Type", "application/json")
                    .header("Set-Cookie", "sid=abc; Path=/"),
            )
            .unwrap();
        });
        let mut session = Session::new();
        let resp = execute("GET", &url, &mut session, &RequestOpts::default()).unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.ok());
        assert!(resp.text().contains("ok"));
        assert!(session.cookies.get("sid").is_some());
        handle.join().unwrap();
    }

    #[test]
    fn local_post_and_download() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let url = format!("http://{addr}/up");
        let handle = thread::spawn(move || {
            let req = server.recv().unwrap();
            assert_eq!(req.method(), "POST");
            assert!(!req.body.is_empty());
            req.respond(OutgoingResponse::from_string("saved").with_status(201))
                .unwrap();
        });
        let mut session = Session::new();
        let mut opts = RequestOpts::default();
        opts.json = Some(r#"{"a":1}"#.into());
        let dir = std::env::temp_dir();
        let path = dir.join("nreq_dl_test.txt");
        let path_s = path.to_string_lossy().to_string();
        let resp = download("POST", &url, &path_s, &mut session, &opts).unwrap();
        assert_eq!(resp.status, 201);
        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, "saved");
        let _ = fs::remove_file(&path);
        handle.join().unwrap();
    }

    #[test]
    fn retry_on_503() {
        use std::sync::{Arc, Mutex};
        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let url = format!("http://{addr}/flaky");
        let hits = Arc::new(Mutex::new(0u32));
        let hits2 = hits.clone();
        let handle = thread::spawn(move || {
            for _ in 0..3 {
                let req = server.recv().unwrap();
                let mut h = hits2.lock().unwrap();
                *h += 1;
                let n = *h;
                drop(h);
                if n < 3 {
                    req.respond(OutgoingResponse::from_string("busy").with_status(503))
                        .unwrap();
                } else {
                    req.respond(OutgoingResponse::from_string("ok").with_status(200))
                        .unwrap();
                }
            }
        });
        let mut session = Session::new();
        session.retries = 5;
        session.backoff_ms = 0;
        let resp = execute("GET", &url, &mut session, &RequestOpts::default()).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(*hits.lock().unwrap(), 3);
        handle.join().unwrap();
    }
}
