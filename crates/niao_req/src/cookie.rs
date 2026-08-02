//! Cookie jar — RFC 6265 subset (name/value/domain/path/secure/expires).

use crate::error::{ReqError, ReqResult};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<u64>,
}

impl Cookie {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            domain: String::new(),
            path: "/".into(),
            secure: false,
            http_only: false,
            expires: None,
        }
    }

    pub fn matches(&self, host: &str, path: &str, is_https: bool) -> bool {
        if self.secure && !is_https {
            return false;
        }
        if let Some(exp) = self.expires {
            if now_secs() >= exp {
                return false;
            }
        }
        if !domain_matches(&self.domain, host) {
            return false;
        }
        path_matches(&self.path, path)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn domain_matches(cookie_domain: &str, host: &str) -> bool {
    if cookie_domain.is_empty() {
        return true;
    }
    let cd = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
    let h = host.to_ascii_lowercase();
    h == cd || h.ends_with(&format!(".{cd}"))
}

fn path_matches(cookie_path: &str, request_path: &str) -> bool {
    let cp = if cookie_path.is_empty() {
        "/"
    } else {
        cookie_path
    };
    request_path == cp
        || (request_path.starts_with(cp)
            && (cp.ends_with('/') || request_path[cp.len()..].starts_with('/')))
}

#[derive(Debug, Clone, Default)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn set(&mut self, cookie: Cookie) {
        self.cookies.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        });
        self.cookies.push(cookie);
    }

    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.iter().rev().find(|c| c.name == name)
    }

    pub fn all(&self) -> &[Cookie] {
        &self.cookies
    }

    pub fn header_for(&self, host: &str, path: &str, is_https: bool) -> Option<String> {
        let mut parts = Vec::new();
        for c in &self.cookies {
            if c.matches(host, path, is_https) {
                parts.push(format!("{}={}", c.name, c.value));
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }

    pub fn store_from_response(
        &mut self,
        set_cookie_headers: &[String],
        request_host: &str,
        request_path: &str,
    ) {
        for raw in set_cookie_headers {
            if let Ok(mut c) = parse_set_cookie(raw) {
                if c.domain.is_empty() {
                    c.domain = request_host.to_string();
                }
                if c.path.is_empty() {
                    c.path = default_path(request_path);
                }
                self.set(c);
            }
        }
    }

    pub fn to_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        for c in &self.cookies {
            m.insert(c.name.clone(), c.value.clone());
        }
        m
    }
}

fn default_path(request_path: &str) -> String {
    if let Some(i) = request_path.rfind('/') {
        if i == 0 {
            "/".into()
        } else {
            request_path[..=i].to_string()
        }
    } else {
        "/".into()
    }
}

/// Parse a single `Set-Cookie` header value.
pub fn parse_set_cookie(header: &str) -> ReqResult<Cookie> {
    let header = header.trim();
    if header.is_empty() {
        return Err(ReqError::Config("empty Set-Cookie".into()));
    }
    let mut parts = header.split(';');
    let nv = parts.next().unwrap_or("").trim();
    let (name, value) = match nv.split_once('=') {
        Some((n, v)) => (n.trim(), v.trim()),
        None => {
            return Err(ReqError::Config(format!(
                "malformed Set-Cookie (no '='): {header}"
            )))
        }
    };
    if name.is_empty() {
        return Err(ReqError::Config("cookie name empty".into()));
    }
    let mut cookie = Cookie::new(name, value);
    for attr in parts {
        let attr = attr.trim();
        if attr.is_empty() {
            continue;
        }
        let (akey, aval) = match attr.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (attr, ""),
        };
        match akey.to_ascii_lowercase().as_str() {
            "domain" => cookie.domain = aval.trim_start_matches('.').to_string(),
            "path" => {
                cookie.path = if aval.is_empty() {
                    "/".into()
                } else {
                    aval.into()
                }
            }
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            "max-age" => {
                if let Ok(secs) = aval.parse::<i64>() {
                    if secs <= 0 {
                        cookie.expires = Some(0);
                    } else {
                        cookie.expires = Some(now_secs().saturating_add(secs as u64));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(cookie)
}

/// Build a `Cookie` request header from a name→value map.
pub fn cookie_header_from_map(map: &HashMap<String, String>) -> String {
    let mut parts: Vec<String> = map.iter().map(|(k, v)| format!("{k}={v}")).collect();
    parts.sort();
    parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_set_cookie() {
        let c = parse_set_cookie("session=abc123; Path=/; HttpOnly").unwrap();
        assert_eq!(c.name, "session");
        assert_eq!(c.value, "abc123");
        assert_eq!(c.path, "/");
        assert!(c.http_only);
    }

    #[test]
    fn jar_matches_path() {
        let mut jar = CookieJar::new();
        let mut c = Cookie::new("a", "1");
        c.domain = "example.com".into();
        c.path = "/api".into();
        jar.set(c);
        assert!(jar.header_for("example.com", "/api/v1", false).is_some());
        assert!(jar.header_for("example.com", "/", false).is_none());
    }

    #[test]
    fn empty_set_cookie_errors() {
        assert!(parse_set_cookie("").is_err());
        assert!(parse_set_cookie("novalue").is_err());
    }
}
