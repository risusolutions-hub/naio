//! Incremental HTTP/1.1 request/response header parser.

use crate::headers::{HeaderMap, MAX_HEADER_BYTES, MAX_HEADER_COUNT, MAX_HEADER_LINE};
use crate::method::Method;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Incomplete,
    InvalidSyntax(String),
    HeaderTooLarge,
    TooManyHeaders,
    Smuggling(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    pub method: Method,
    pub target: String,
    pub version: (u8, u8),
    pub headers: HeaderMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHead {
    pub version: (u8, u8),
    pub status: u16,
    pub reason: String,
    pub headers: HeaderMap,
}

fn find_crlf(buf: &[u8], start: usize) -> Option<usize> {
    buf[start..]
        .windows(2)
        .position(|w| w == b"\r\n")
        .map(|i| start + i)
}

fn parse_version(s: &str) -> Result<(u8, u8), ParseError> {
    let s = s
        .strip_prefix("HTTP/")
        .ok_or_else(|| ParseError::InvalidSyntax("bad version".into()))?;
    let mut parts = s.split('.');
    let major: u8 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| ParseError::InvalidSyntax("bad version major".into()))?;
    let minor: u8 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| ParseError::InvalidSyntax("bad version minor".into()))?;
    Ok((major, minor))
}

fn parse_header_line(line: &str, headers: &mut HeaderMap) -> Result<(), ParseError> {
    if line.is_empty() {
        return Ok(());
    }
    if line.len() > MAX_HEADER_LINE {
        return Err(ParseError::HeaderTooLarge);
    }
    if line.as_bytes().contains(&b'\t') {
        return Err(ParseError::InvalidSyntax("obs-fold not allowed".into()));
    }
    let Some((name, value)) = line.split_once(':') else {
        return Err(ParseError::InvalidSyntax("missing colon in header".into()));
    };
    if name.is_empty() || name.as_bytes().iter().any(|&b| b == b' ') {
        return Err(ParseError::InvalidSyntax("invalid header name".into()));
    }
    let value = value.trim_start();
    let key_lower = name.to_ascii_lowercase();
    if key_lower == "content-length" && headers.get("content-length").is_some() {
        return Err(ParseError::Smuggling("dual Content-Length".into()));
    }
    headers
        .append_raw(name, value)
        .map_err(|_| ParseError::TooManyHeaders)?;
    Ok(())
}

fn validate_smuggling(headers: &HeaderMap) -> Result<(), ParseError> {
    let mut cl_count = 0usize;
    let mut has_te = false;
    for (name, value) in headers.iter() {
        if name == "content-length" {
            cl_count += 1;
        }
        if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            has_te = true;
        }
    }
    if cl_count > 1 {
        return Err(ParseError::Smuggling("dual Content-Length".into()));
    }
    if cl_count > 0 && has_te {
        return Err(ParseError::Smuggling(
            "Content-Length with Transfer-Encoding: chunked".into(),
        ));
    }
    Ok(())
}

fn parse_headers(buf: &[u8], after_request_line: usize, header_end: usize) -> Result<HeaderMap, ParseError> {
    if header_end > MAX_HEADER_BYTES {
        return Err(ParseError::HeaderTooLarge);
    }
    let section = std::str::from_utf8(&buf[after_request_line..header_end])
        .map_err(|_| ParseError::InvalidSyntax("invalid utf8 in headers".into()))?;
    let mut headers = HeaderMap::new();
    for line in section.split("\r\n") {
        if line.is_empty() {
            continue;
        }
        parse_header_line(line, &mut headers)?;
        if headers.len() > MAX_HEADER_COUNT {
            return Err(ParseError::TooManyHeaders);
        }
    }
    validate_smuggling(&headers)?;
    Ok(headers)
}

fn find_header_end(buf: &[u8]) -> Result<Option<usize>, ParseError> {
    if buf.len() > MAX_HEADER_BYTES + 256 {
        return Err(ParseError::HeaderTooLarge);
    }
    Ok(buf.windows(4).position(|w| w == b"\r\n\r\n"))
}

/// Parse an HTTP request from `buf`. Returns bytes consumed (including header terminator).
pub fn parse_request(buf: &[u8]) -> Result<(RequestHead, usize), ParseError> {
    let Some(header_end) = find_header_end(buf)? else {
        return Err(ParseError::Incomplete);
    };
    let line_end = find_crlf(buf, 0).ok_or(ParseError::Incomplete)?;
    let request_line = std::str::from_utf8(&buf[..line_end])
        .map_err(|_| ParseError::InvalidSyntax("invalid utf8 request line".into()))?;
    let mut parts = request_line.split(' ');
    let method_s = parts
        .next()
        .ok_or_else(|| ParseError::InvalidSyntax("missing method".into()))?;
    let method = Method::parse(method_s)
        .ok_or_else(|| ParseError::InvalidSyntax(format!("unknown method {method_s}")))?;
    let target = parts
        .next()
        .ok_or_else(|| ParseError::InvalidSyntax("missing target".into()))?
        .to_string();
    let version_s = parts
        .next()
        .ok_or_else(|| ParseError::InvalidSyntax("missing version".into()))?;
    if parts.next().is_some() {
        return Err(ParseError::InvalidSyntax(
            "extra request line tokens".into(),
        ));
    }
    let version = parse_version(version_s)?;
    let headers = parse_headers(buf, line_end + 2, header_end)?;
    let consumed = header_end + 4;
    Ok((
        RequestHead {
            method,
            target,
            version,
            headers,
        },
        consumed,
    ))
}

/// Parse an HTTP response from `buf`.
pub fn parse_response(buf: &[u8]) -> Result<(ResponseHead, usize), ParseError> {
    let Some(header_end) = find_header_end(buf)? else {
        return Err(ParseError::Incomplete);
    };
    let line_end = find_crlf(buf, 0).ok_or(ParseError::Incomplete)?;
    let status_line = std::str::from_utf8(&buf[..line_end])
        .map_err(|_| ParseError::InvalidSyntax("invalid utf8 status line".into()))?;
    let mut parts = status_line.splitn(3, ' ');
    let version_s = parts
        .next()
        .ok_or_else(|| ParseError::InvalidSyntax("missing version".into()))?;
    let version = parse_version(version_s)?;
    let status: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ParseError::InvalidSyntax("missing status".into()))?;
    let reason = parts.next().unwrap_or("").to_string();
    let headers = parse_headers(buf, line_end + 2, header_end)?;
    let consumed = header_end + 4;
    Ok((
        ResponseHead {
            version,
            status,
            reason,
            headers,
        },
        consumed,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyMode {
    None,
    Fixed(usize),
    Chunked,
}

pub fn body_mode(headers: &HeaderMap, method: Option<Method>) -> Result<BodyMode, ParseError> {
    if method == Some(Method::Head) {
        return Ok(BodyMode::None);
    }
    if let Some(te) = headers.get("transfer-encoding") {
        if te.to_ascii_lowercase().contains("chunked") {
            return Ok(BodyMode::Chunked);
        }
    }
    if let Some(cl) = headers.get("content-length") {
        let n: usize = cl
            .parse()
            .map_err(|_| ParseError::InvalidSyntax("invalid Content-Length".into()))?;
        return Ok(BodyMode::Fixed(n));
    }
    Ok(BodyMode::None)
}

pub fn decode_chunked(data: &[u8]) -> Result<(Vec<u8>, usize), ParseError> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        let Some(line_end) = find_crlf(data, pos) else {
            return Err(ParseError::Incomplete);
        };
        let line = std::str::from_utf8(&data[pos..line_end])
            .map_err(|_| ParseError::InvalidSyntax("chunk size utf8".into()))?;
        let size_str = line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16)
            .map_err(|_| ParseError::InvalidSyntax("bad chunk size".into()))?;
        pos = line_end + 2;
        if size == 0 {
            while pos + 1 < data.len() {
                if let Some(trailer_end) = find_crlf(data, pos) {
                    if trailer_end == pos {
                        return Ok((out, pos + 2));
                    }
                    pos = trailer_end + 2;
                } else {
                    break;
                }
            }
            return Ok((out, pos));
        }
        if pos + size + 2 > data.len() {
            return Err(ParseError::Incomplete);
        }
        out.extend_from_slice(&data[pos..pos + size]);
        pos += size;
        if data.get(pos..pos + 2) != Some(b"\r\n") {
            return Err(ParseError::InvalidSyntax("chunk missing CRLF".into()));
        }
        pos += 2;
    }
}

pub fn read_body(mode: BodyMode, buf: &[u8], offset: usize) -> Result<(Vec<u8>, usize), ParseError> {
    match mode {
        BodyMode::None => Ok((Vec::new(), offset)),
        BodyMode::Fixed(n) => {
            if buf.len() < offset + n {
                return Err(ParseError::Incomplete);
            }
            Ok((buf[offset..offset + n].to_vec(), offset + n))
        }
        BodyMode::Chunked => {
            let (body, end) = decode_chunked(&buf[offset..])?;
            Ok((body, offset + end))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_request() {
        let req = b"GET /hello HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let (head, n) = parse_request(req).unwrap();
        assert_eq!(head.method, Method::Get);
        assert_eq!(head.target, "/hello");
        assert_eq!(n, req.len());
    }

    #[test]
    fn rejects_dual_content_length() {
        let req = b"POST / HTTP/1.1\r\nContent-Length: 4\r\nContent-Length: 5\r\n\r\n";
        assert!(matches!(parse_request(req), Err(ParseError::Smuggling(_))));
    }

    #[test]
    fn rejects_cl_and_te() {
        let req = b"POST / HTTP/1.1\r\nContent-Length: 5\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert!(matches!(parse_request(req), Err(ParseError::Smuggling(_))));
    }

    #[test]
    fn rejects_obs_fold() {
        let req = b"GET / HTTP/1.1\r\nFolded: value\r\n\tcontinued\r\n\r\n";
        assert!(matches!(
            parse_request(req),
            Err(ParseError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn incomplete_when_truncated() {
        let req = b"GET / HTTP/1.1\r\nHost: x\r\n";
        assert_eq!(parse_request(req), Err(ParseError::Incomplete));
    }

    #[test]
    fn chunked_roundtrip() {
        let raw = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        let (body, n) = decode_chunked(raw).unwrap();
        assert_eq!(body, b"hello world");
        assert_eq!(n, raw.len());
    }
}
