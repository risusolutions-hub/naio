//! Session configuration and state.

use crate::cookie::CookieJar;
use std::collections::HashMap;

/// Default User-Agent for nreq.
pub const DEFAULT_USER_AGENT: &str = "nreq/0.1 (+https://niao.dev; requests-compatible)";

#[derive(Debug, Clone)]
pub struct Session {
    pub base_url: String,
    pub headers: HashMap<String, String>,
    pub cookies: CookieJar,
    pub auth: Option<(String, String)>,
    pub bearer: Option<String>,
    pub timeout_ms: u64,
    pub max_redirects: u8,
    pub allow_redirects: bool,
    pub retries: u32,
    pub retry_statuses: Vec<u16>,
    pub backoff_ms: u64,
    pub proxy: Option<String>,
    pub user_agent: String,
    pub params: HashMap<String, String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            headers: HashMap::new(),
            cookies: CookieJar::new(),
            auth: None,
            bearer: None,
            timeout_ms: 30_000,
            max_redirects: 10,
            allow_redirects: true,
            retries: 0,
            retry_statuses: vec![408, 429, 500, 502, 503, 504],
            backoff_ms: 100,
            proxy: None,
            user_agent: DEFAULT_USER_AGENT.into(),
            params: HashMap::new(),
        }
    }
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
}

/// Per-request options merged over a session.
#[derive(Debug, Clone, Default)]
pub struct RequestOpts {
    pub headers: HashMap<String, String>,
    pub params: HashMap<String, String>,
    pub data: Option<String>,
    pub json: Option<String>,
    pub body_bytes: Option<Vec<u8>>,
    pub content_type: Option<String>,
    pub auth: Option<(String, String)>,
    pub bearer: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_redirects: Option<u8>,
    pub allow_redirects: Option<bool>,
    pub retries: Option<u32>,
    pub retry_statuses: Option<Vec<u16>>,
    pub backoff_ms: Option<u64>,
    pub proxy: Option<String>,
    pub user_agent: Option<String>,
    pub cookies: HashMap<String, String>,
    pub files: Vec<crate::multipart::MultipartPart>,
    pub stream_path: Option<String>,
}

impl RequestOpts {
    pub fn merge_timeout(&self, session: &Session) -> u64 {
        self.timeout_ms.unwrap_or(session.timeout_ms)
    }

    pub fn merge_retries(&self, session: &Session) -> u32 {
        self.retries.unwrap_or(session.retries)
    }

    pub fn merge_backoff(&self, session: &Session) -> u64 {
        self.backoff_ms.unwrap_or(session.backoff_ms)
    }

    pub fn merge_max_redirects(&self, session: &Session) -> u8 {
        self.max_redirects.unwrap_or(session.max_redirects)
    }

    pub fn merge_allow_redirects(&self, session: &Session) -> bool {
        self.allow_redirects.unwrap_or(session.allow_redirects)
    }

    pub fn merge_proxy<'a>(&'a self, session: &'a Session) -> Option<&'a str> {
        self.proxy
            .as_deref()
            .or(session.proxy.as_deref())
            .filter(|s| !s.is_empty())
    }
}
