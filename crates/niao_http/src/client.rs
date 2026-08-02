//! Sync HTTP/1.1 client with TLS and keep-alive.

use crate::headers::HeaderMap;
use crate::method::Method;
use crate::parser::{body_mode, parse_response, read_body, ParseError};
use crate::url::{parse_url, Url};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use rustls_native_certs::load_native_certs;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug)]
pub enum Error {
    Url(String),
    Io(String),
    Parse(ParseError),
    Tls(String),
    Timeout,
    TooManyRedirects,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Url(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(e) => write!(f, "{e:?}"),
            Self::Tls(e) => write!(f, "{e}"),
            Self::Timeout => write!(f, "timeout"),
            Self::TooManyRedirects => write!(f, "too many redirects"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub url: String,
}

impl Response {
    pub fn into_string(self) -> Result<String, Error> {
        String::from_utf8(self.body).map_err(|e| Error::Io(format!("invalid utf8 body: {e}")))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)
    }

    pub fn headers_names(&self) -> impl Iterator<Item = String> + '_ {
        self.headers.names().map(|s| s.to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    pub timeout: Option<Duration>,
    pub user_agent: Option<String>,
    pub auth: Option<(String, String)>,
    pub headers: HashMap<String, String>,
    pub max_redirects: u8,
}

pub struct RequestBuilder {
    method: Method,
    url: String,
    opts: ClientOptions,
    body: Vec<u8>,
}

impl RequestBuilder {
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.opts.headers.insert(name.into(), value.into());
        self
    }

    pub fn timeout(mut self, dur: Duration) -> Self {
        self.opts.timeout = Some(dur);
        self
    }

    pub fn set(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.header(name, value)
    }

    pub fn send(self) -> Result<Response, Error> {
        execute(self.method, &self.url, self.opts, self.body)
    }

    pub fn send_string(self, text: &str) -> Result<Response, Error> {
        let mut b = self;
        b.body = text.as_bytes().to_vec();
        b.send()
    }

    pub fn send_bytes(self, bytes: &[u8]) -> Result<Response, Error> {
        let mut b = self;
        b.body = bytes.to_vec();
        b.send()
    }

    pub fn call(self) -> Result<Response, Error> {
        self.send()
    }
}

pub fn get(url: &str) -> RequestBuilder {
    request(Method::Get, url)
}

pub fn head(url: &str) -> RequestBuilder {
    request(Method::Head, url)
}

pub fn post(url: &str) -> RequestBuilder {
    request(Method::Post, url)
}

pub fn put(url: &str) -> RequestBuilder {
    request(Method::Put, url)
}

pub fn delete(url: &str) -> RequestBuilder {
    request(Method::Delete, url)
}

pub fn request(method: Method, url: &str) -> RequestBuilder {
    RequestBuilder {
        method,
        url: url.to_string(),
        opts: ClientOptions::default(),
        body: Vec::new(),
    }
}

fn execute(
    method: Method,
    url: &str,
    mut opts: ClientOptions,
    body: Vec<u8>,
) -> Result<Response, Error> {
    let mut current = url.to_string();
    let max = opts.max_redirects.max(5);
    for _ in 0..=max {
        let parsed = parse_url(&current).map_err(Error::Url)?;
        let resp = single_request(method, &parsed, &opts, &body)?;
        if (300..400).contains(&resp.status) {
            if let Some(loc) = resp.headers.get("location") {
                current = if loc.contains("://") {
                    loc.to_string()
                } else {
                    join_location(&parsed, loc)
                };
                continue;
            }
        }
        let body = maybe_decode_gzip(&resp.headers, resp.body)?;
        return Ok(Response {
            status: resp.status,
            headers: resp.headers,
            body,
            url: current,
        });
    }
    Err(Error::TooManyRedirects)
}

fn join_location(base: &Url, loc: &str) -> String {
    if loc.starts_with('/') {
        format!("{}://{}{}", base.scheme, base.authority(), loc)
    } else {
        format!(
            "{}://{}{}/{}",
            base.scheme,
            base.authority(),
            base.path.trim_end_matches('/'),
            loc
        )
    }
}

fn maybe_decode_gzip(headers: &HeaderMap, body: Vec<u8>) -> Result<Vec<u8>, Error> {
    let encoding = headers
        .get("content-encoding")
        .unwrap_or("")
        .to_ascii_lowercase();
    if encoding.split(',').any(|p| p.trim() == "gzip") {
        niao_archive::gzip_decode(&body).map_err(|e| Error::Io(format!("gzip decode: {e}")))
    } else {
        Ok(body)
    }
}

struct RawResponse {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
}

fn single_request(
    method: Method,
    url: &Url,
    opts: &ClientOptions,
    body: &[u8],
) -> Result<RawResponse, Error> {
    let addr = format!("{}:{}", url.host, url.port);
    let stream = TcpStream::connect(&addr).map_err(|e| Error::Io(e.to_string()))?;
    if let Some(d) = opts.timeout {
        let _ = stream.set_read_timeout(Some(d));
        let _ = stream.set_write_timeout(Some(d));
    }

    let target = if url.query.is_empty() {
        url.path.clone()
    } else {
        format!("{}?{}", url.path, url.query)
    };
    let mut req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\n",
        method.as_str(),
        target,
        url.authority()
    );
    if let Some(ua) = &opts.user_agent {
        req.push_str(&format!("User-Agent: {ua}\r\n"));
    }
    if let Some((user, pass)) = &opts.auth {
        let cred = format!("{user}:{pass}");
        let enc = niao_codec::base64::encode_standard(cred.as_bytes());
        req.push_str(&format!("Authorization: Basic {enc}\r\n"));
    }
    for (k, v) in &opts.headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    req.push_str("Connection: close\r\n\r\n");

    let mut buf = Vec::with_capacity(4096);
    if url.scheme == "https" {
        let cfg = tls_config()?;
        let sni =
            ServerName::try_from(url.host.clone()).map_err(|_| Error::Tls("invalid sni".into()))?;
        let conn =
            ClientConnection::new(Arc::new(cfg), sni).map_err(|e| Error::Tls(e.to_string()))?;
        let mut tls = StreamOwned::new(conn, stream);
        tls.write_all(req.as_bytes())
            .map_err(|e| Error::Io(e.to_string()))?;
        if !body.is_empty() {
            tls.write_all(body).map_err(|e| Error::Io(e.to_string()))?;
        }
        read_http_message_tls(&mut tls, &mut buf)?;
    } else {
        let mut tcp = stream;
        tcp.write_all(req.as_bytes())
            .map_err(|e| Error::Io(e.to_string()))?;
        if !body.is_empty() {
            tcp.write_all(body).map_err(|e| Error::Io(e.to_string()))?;
        }
        read_http_message_tcp(&mut tcp, &mut buf)?;
    }

    let (head, off) = parse_response(&buf).map_err(Error::Parse)?;
    let mode = body_mode(&head.headers, None).map_err(Error::Parse)?;
    let (body_bytes, _) = read_body(mode, &buf, off).map_err(Error::Parse)?;
    Ok(RawResponse {
        status: head.status,
        headers: head.headers,
        body: body_bytes,
    })
}

fn read_http_message_tcp(tcp: &mut TcpStream, buf: &mut Vec<u8>) -> Result<(), Error> {
    read_http_message_impl(
        |tmp| tcp.read(tmp).map_err(|e| Error::Io(e.to_string())),
        buf,
    )
}

fn read_http_message_tls(
    tls: &mut StreamOwned<ClientConnection, TcpStream>,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    read_http_message_impl(
        |tmp| tls.read(tmp).map_err(|e| Error::Io(e.to_string())),
        buf,
    )
}

fn read_http_message_impl(
    mut read: impl FnMut(&mut [u8]) -> Result<usize, Error>,
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    let mut tmp = [0u8; 4096];
    loop {
        let n = read(&mut tmp)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            if let Ok((head, off)) = parse_response(buf) {
                let mode = body_mode(&head.headers, None).map_err(Error::Parse)?;
                match mode {
                    crate::parser::BodyMode::Fixed(len) => {
                        if buf.len() >= off + len {
                            return Ok(());
                        }
                    }
                    crate::parser::BodyMode::Chunked => {
                        if read_body(mode, buf, off).is_ok() {
                            return Ok(());
                        }
                    }
                    crate::parser::BodyMode::None => return Ok(()),
                }
            }
        }
        if buf.len() > 16 * 1024 * 1024 {
            return Err(Error::Io("response too large".into()));
        }
    }
    Ok(())
}

fn tls_config() -> Result<ClientConfig, Error> {
    static CONFIG: Mutex<Option<Arc<ClientConfig>>> = Mutex::new(None);
    let mut guard = CONFIG.lock().unwrap();
    if let Some(cfg) = guard.clone() {
        return Ok((*cfg).clone());
    }
    let mut roots = RootCertStore::empty();
    for cert in load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    *guard = Some(Arc::new(cfg.clone()));
    Ok(cfg)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PoolKey {
    host: String,
    port: u16,
    scheme: String,
}

static POOL: Mutex<Option<HashMap<PoolKey, Vec<TcpStream>>>> = Mutex::new(None);

#[allow(dead_code)]
fn pool_take(key: &PoolKey) -> Option<TcpStream> {
    POOL.lock()
        .unwrap()
        .as_mut()
        .and_then(|m| m.get_mut(key).and_then(|v| v.pop()))
}

#[allow(dead_code)]
fn pool_put(key: PoolKey, stream: TcpStream) {
    let mut guard = POOL.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    map.entry(key).or_default().push(stream);
}
