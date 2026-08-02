//! Response value returned to callers.

use crate::error::{ReqError, ReqResult};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
    pub reason: String,
    pub set_cookies: Vec<String>,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json_value(&self) -> ReqResult<niao_json_core::Value> {
        let s = std::str::from_utf8(&self.body).map_err(|e| ReqError::Json(e.to_string()))?;
        niao_json_core::parse(s).map_err(|e| ReqError::Json(e.to_string()))
    }

    pub fn raise_for_status(&self) -> ReqResult<()> {
        if self.ok() {
            Ok(())
        } else {
            let msg = self.text();
            let preview: String = msg.chars().take(200).collect();
            Err(ReqError::Status {
                status: self.status,
                message: if preview.is_empty() {
                    reason_phrase(self.status).into()
                } else {
                    preview
                },
            })
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_ascii_lowercase();
        self.headers.get(&lower).map(String::as_str)
    }
}

pub fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

pub fn from_http(resp: niao_http::Response, elapsed_ms: u64) -> Response {
    let status = resp.status;
    let mut headers = HashMap::new();
    let mut set_cookies = Vec::new();
    for name in resp.headers_names() {
        if let Some(v) = resp.header(&name) {
            let key = name.to_ascii_lowercase();
            if key == "set-cookie" {
                set_cookies.push(v.to_string());
            }
            headers.insert(key, v.to_string());
        }
    }
    Response {
        status,
        url: resp.url,
        headers,
        body: resp.body,
        elapsed_ms,
        reason: reason_phrase(status).into(),
        set_cookies,
    }
}
