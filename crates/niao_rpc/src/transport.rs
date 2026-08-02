//! Sync stdio / TCP / HTTP transports for JSON-RPC 2.0.

use crate::codec::{decode, encode, MAX_BYTES};
use crate::dispatch::{dispatch_str, MethodResult};
use crate::error::EngineError;
use crate::frame::{frame_text, unframe, FrameStyle};
use crate::message::{Id, Message, Request, Response};
use niao_json_core::{to_string, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Options for stream-oriented transports.
#[derive(Clone, Debug)]
pub struct TransportOptions {
    pub style: FrameStyle,
    pub timeout: Option<Duration>,
    pub max_requests: usize,
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            style: FrameStyle::Ndjson,
            timeout: Some(Duration::from_secs(30)),
            max_requests: 10_000,
        }
    }
}

/// Process one complete JSON-RPC payload string through a handler and return
/// the serialized response (empty string for notifications with no reply).
pub fn handle_payload<F>(input: &str, mut call: F) -> String
where
    F: FnMut(&str, Option<&Value>) -> MethodResult,
{
    let out = dispatch_str(input, &mut call);
    if out.is_null() {
        String::new()
    } else {
        to_string(&out)
    }
}

/// Read framed messages from `reader`, dispatch each request, write responses.
pub fn serve_stream<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    opts: &TransportOptions,
    mut call: F,
) -> Result<usize, EngineError>
where
    R: Read,
    W: Write,
    F: FnMut(&str, Option<&Value>) -> MethodResult,
{
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 8192];
    let mut handled = 0usize;
    let mut pending = String::new();

    loop {
        if handled >= opts.max_requests {
            break;
        }
        let n = reader
            .read(&mut chunk)
            .map_err(|e| EngineError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        if buf.len() + n > MAX_BYTES * 2 {
            return Err(EngineError::Limit("stream buffer overflow".into()));
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = std::str::from_utf8(&buf)
            .map_err(|_| EngineError::Parse("stream contains invalid UTF-8".into()))?;
        // Combine with any leftover from previous iteration via pending.
        let combined = if pending.is_empty() {
            text.to_string()
        } else {
            pending.clone() + text
        };
        let ur = unframe(&combined, opts.style)?;
        pending = ur.rest;
        buf.clear();
        // Keep rest as bytes for next read.
        buf.extend_from_slice(pending.as_bytes());
        pending.clear();

        for msg in ur.messages {
            handled += 1;
            let input = encode(&msg);
            let out = handle_payload(&input, &mut call);
            if !out.is_empty() {
                let framed = frame_text(&out, opts.style);
                writer
                    .write_all(framed.as_bytes())
                    .map_err(|e| EngineError::Io(e.to_string()))?;
                writer.flush().map_err(|e| EngineError::Io(e.to_string()))?;
            }
            if handled >= opts.max_requests {
                break;
            }
        }
    }
    Ok(handled)
}

fn set_timeouts(stream: &TcpStream, opts: &TransportOptions) -> Result<(), EngineError> {
    if let Some(t) = opts.timeout {
        stream
            .set_read_timeout(Some(t))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        stream
            .set_write_timeout(Some(t))
            .map_err(|e| EngineError::Io(e.to_string()))?;
    }
    Ok(())
}

/// Accept one TCP connection, serve up to `max_requests`, then return.
pub fn tcp_serve_once<A, F>(addr: A, opts: &TransportOptions, call: F) -> Result<usize, EngineError>
where
    A: ToSocketAddrs,
    F: FnMut(&str, Option<&Value>) -> MethodResult,
{
    let listener = TcpListener::bind(addr).map_err(|e| EngineError::Transport(e.to_string()))?;
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| EngineError::Transport(e.to_string()))?;
    set_timeouts(&stream, opts)?;
    let mut writer = stream
        .try_clone()
        .map_err(|e| EngineError::Io(e.to_string()))?;
    serve_stream(&mut stream, &mut writer, opts, call)
}

/// Connect to a TCP JSON-RPC peer, send one request, read one framed response.
pub fn tcp_call<A>(
    addr: A,
    method: &str,
    params: Option<Value>,
    id: Id,
    opts: &TransportOptions,
) -> Result<Response, EngineError>
where
    A: ToSocketAddrs,
{
    let mut stream = TcpStream::connect(addr).map_err(|e| EngineError::Transport(e.to_string()))?;
    set_timeouts(&stream, opts)?;
    let req = Message::Request(Request::call(method, params, id));
    let framed = crate::frame::frame(&req, opts.style);
    stream
        .write_all(framed.as_bytes())
        .map_err(|e| EngineError::Io(e.to_string()))?;
    stream.flush().map_err(|e| EngineError::Io(e.to_string()))?;

    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut pending = String::new();
    loop {
        let n = stream
            .read(&mut chunk)
            .map_err(|e| EngineError::Io(e.to_string()))?;
        if n == 0 {
            return Err(EngineError::Transport(
                "connection closed before response".into(),
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = std::str::from_utf8(&buf)
            .map_err(|_| EngineError::Parse("invalid UTF-8 in response".into()))?;
        let combined = if pending.is_empty() {
            text.to_string()
        } else {
            pending + text
        };
        let ur = unframe(&combined, opts.style)?;
        if let Some(msg) = ur.messages.into_iter().next() {
            match msg {
                Message::Response(r) => return Ok(r),
                other => {
                    // Tolerate a request incorrectly — try decode as response value.
                    let v = other.to_value();
                    return crate::message::parse_response_value(&v);
                }
            }
        }
        pending = ur.rest;
        buf.clear();
        buf.extend_from_slice(pending.as_bytes());
        pending.clear();
    }
}

/// Parse HTTP request bytes; return (path, body) for POST.
fn parse_http_request(raw: &str) -> Result<(String, String), EngineError> {
    let (header_part, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| EngineError::Framing("incomplete HTTP request".into()))?;
    let mut lines = header_part.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| EngineError::Framing("missing request line".into()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/").to_string();
    if method != "POST" && method != "GET" {
        return Err(EngineError::Transport(format!(
            "unsupported HTTP method {method}"
        )));
    }
    let mut content_length = body.len();
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v
                .trim()
                .parse()
                .map_err(|_| EngineError::Framing("invalid Content-Length".into()))?;
        }
    }
    if content_length > MAX_BYTES {
        return Err(EngineError::Limit("HTTP body too large".into()));
    }
    let body = if body.len() >= content_length {
        body[..content_length].to_string()
    } else {
        body.to_string()
    };
    Ok((path, body))
}

fn http_response(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Accept one HTTP connection and handle a single JSON-RPC POST.
pub fn http_serve_once<A, F>(addr: A, path_filter: &str, call: F) -> Result<(), EngineError>
where
    A: ToSocketAddrs,
    F: FnMut(&str, Option<&Value>) -> MethodResult,
{
    let listener = TcpListener::bind(addr).map_err(|e| EngineError::Transport(e.to_string()))?;
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| EngineError::Transport(e.to_string()))?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = stream
            .read(&mut chunk)
            .map_err(|e| EngineError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            // Check if we have full body via Content-Length.
            if let Ok(text) = std::str::from_utf8(&buf) {
                if let Ok((_, _)) = parse_http_request(text) {
                    // May still need more body bytes — parse_http_request uses whatever is present.
                    if let Some((headers, _)) = text.split_once("\r\n\r\n") {
                        let mut need = 0usize;
                        for line in headers.split("\r\n").skip(1) {
                            let lower = line.to_ascii_lowercase();
                            if let Some(v) = lower.strip_prefix("content-length:") {
                                need = v.trim().parse().unwrap_or(0);
                            }
                        }
                        let body_start = headers.len() + 4;
                        if buf.len() >= body_start + need {
                            break;
                        }
                    }
                }
            }
        }
        if buf.len() > MAX_BYTES + 4096 {
            return Err(EngineError::Limit("HTTP request too large".into()));
        }
    }
    let raw = std::str::from_utf8(&buf)
        .map_err(|_| EngineError::Parse("invalid UTF-8 in HTTP request".into()))?;
    let (path, body) = parse_http_request(raw)?;
    if path_filter != "*" && path != path_filter {
        let resp = http_response(404, "Not Found", "{\"error\":\"not found\"}");
        stream
            .write_all(resp.as_bytes())
            .map_err(|e| EngineError::Io(e.to_string()))?;
        return Ok(());
    }
    let mut call = call;
    let out = handle_payload(&body, &mut call);
    let (status, reason, body_out) = if out.is_empty() {
        // Notification — 204
        (204, "No Content", "")
    } else {
        (200, "OK", out.as_str())
    };
    let resp = http_response(status, reason, body_out);
    stream
        .write_all(resp.as_bytes())
        .map_err(|e| EngineError::Io(e.to_string()))?;
    Ok(())
}

/// HTTP POST JSON-RPC call. `url` form: `http://host:port/path`.
pub fn http_call(
    url: &str,
    method: &str,
    params: Option<Value>,
    id: Id,
) -> Result<Response, EngineError> {
    let (host, port, path) = parse_http_url(url)?;
    let req_msg = Message::Request(Request::call(method, params, id));
    let body = encode(&req_msg);
    let http_req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect((host.as_str(), port))
        .map_err(|e| EngineError::Transport(e.to_string()))?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
    stream
        .write_all(http_req.as_bytes())
        .map_err(|e| EngineError::Io(e.to_string()))?;
    stream.flush().map_err(|e| EngineError::Io(e.to_string()))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| EngineError::Io(e.to_string()))?;
    let raw = std::str::from_utf8(&buf)
        .map_err(|_| EngineError::Parse("invalid UTF-8 in HTTP response".into()))?;
    let (_status, resp_body) = parse_http_response(raw)?;
    if resp_body.trim().is_empty() {
        return Err(EngineError::Transport("empty HTTP response body".into()));
    }
    match decode(&resp_body)? {
        Message::Response(r) => Ok(r),
        other => crate::message::parse_response_value(&other.to_value()),
    }
}

fn parse_http_url(url: &str) -> Result<(String, u16, String), EngineError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| EngineError::Invalid("only http:// URLs are supported".into()))?;
    let (hostport, path) = match rest.split_once('/') {
        Some((hp, p)) => (hp, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = if let Some((h, p)) = hostport.split_once(':') {
        let port: u16 = p
            .parse()
            .map_err(|_| EngineError::Invalid("invalid port".into()))?;
        (h.to_string(), port)
    } else {
        (hostport.to_string(), 80)
    };
    Ok((host, port, path))
}

fn parse_http_response(raw: &str) -> Result<(u16, String), EngineError> {
    let (header_part, body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| EngineError::Framing("incomplete HTTP response".into()))?;
    let status_line = header_part
        .lines()
        .next()
        .ok_or_else(|| EngineError::Framing("missing status line".into()))?;
    let mut parts = status_line.split_whitespace();
    let _http = parts.next();
    let status: u16 = parts
        .next()
        .ok_or_else(|| EngineError::Framing("missing status code".into()))?
        .parse()
        .map_err(|_| EngineError::Framing("bad status code".into()))?;
    let mut content_length = body.len();
    for line in header_part.lines().skip(1) {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(body.len());
        }
    }
    let body = if body.len() >= content_length {
        body[..content_length].to_string()
    } else {
        body.to_string()
    };
    if !(200..300).contains(&status) && status != 204 {
        return Err(EngineError::Transport(format!(
            "HTTP status {status}: {body}"
        )));
    }
    Ok((status, body))
}

/// Stdio helper: treat `input` as one NDJSON / raw JSON payload and return framed output.
pub fn stdio_exchange<F>(input: &str, style: FrameStyle, mut call: F) -> String
where
    F: FnMut(&str, Option<&Value>) -> MethodResult,
{
    let trimmed = input.trim_end_matches(['\n', '\r']);
    let out = handle_payload(trimmed, &mut call);
    if out.is_empty() {
        String::new()
    } else {
        frame_text(&out, style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RpcError;
    use std::io::Cursor;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn stdio_exchange_ping() {
        let out = stdio_exchange(
            r#"{"jsonrpc":"2.0","method":"ping","id":1}"#,
            FrameStyle::Ndjson,
            |m, _| {
                if m == "ping" {
                    Ok(Value::string("pong"))
                } else {
                    Err(RpcError::method_not_found(m))
                }
            },
        );
        assert!(out.contains("pong"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn tcp_roundtrip() {
        let barrier = Arc::new(Barrier::new(2));
        let b2 = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            let opts = TransportOptions {
                max_requests: 1,
                ..TransportOptions::default()
            };
            b2.wait();
            tcp_serve_once("127.0.0.1:19870", &opts, |m, p| {
                if m == "add" {
                    let a = p
                        .and_then(|v| match v {
                            Value::Array(xs) => xs.first().and_then(|x| x.as_i64()),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let b = p
                        .and_then(|v| match v {
                            Value::Array(xs) => xs.get(1).and_then(|x| x.as_i64()),
                            _ => None,
                        })
                        .unwrap_or(0);
                    Ok(Value::int(a + b))
                } else {
                    Err(RpcError::method_not_found(m))
                }
            })
        });
        barrier.wait();
        thread::sleep(Duration::from_millis(50));
        let opts = TransportOptions::default();
        let resp = tcp_call(
            "127.0.0.1:19870",
            "add",
            Some(Value::array(vec![Value::int(2), Value::int(3)])),
            Id::Number(1),
            &opts,
        )
        .unwrap();
        match resp.body {
            crate::message::ResponseBody::Success(v) => assert_eq!(v.as_i64(), Some(5)),
            _ => panic!("expected success"),
        }
        handle.join().unwrap().unwrap();
    }

    #[test]
    fn serve_stream_cursor() {
        let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"echo\",\"params\":[\"hi\"],\"id\":1}\n";
        let mut reader = Cursor::new(&input[..]);
        let mut writer = Cursor::new(Vec::new());
        let opts = TransportOptions {
            max_requests: 1,
            ..TransportOptions::default()
        };
        let n = serve_stream(&mut reader, &mut writer, &opts, |m, p| {
            assert_eq!(m, "echo");
            Ok(p.cloned().unwrap_or(Value::Null))
        })
        .unwrap();
        assert_eq!(n, 1);
        let s = String::from_utf8(writer.into_inner()).unwrap();
        assert!(s.contains("hi"));
    }
}
