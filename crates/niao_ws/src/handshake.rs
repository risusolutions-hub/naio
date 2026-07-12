//! WebSocket opening handshake (RFC 6455).

use crate::error::WsError;
use niao_crypto::sha1;
use niao_http::parse_request;
use std::io::{Read, Write};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn ws_accept_key(key: &str) -> String {
    let mut input = String::with_capacity(key.len() + GUID.len());
    input.push_str(key);
    input.push_str(GUID);
    niao_codec::base64::encode_standard(&sha1(input.as_bytes()))
}

pub fn client_request(host: &str, path: &str, key: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    )
}

pub fn generate_key() -> String {
    let mut bytes = [0u8; 16];
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = ((t >> (i * 4)) & 0xFF) as u8;
    }
    niao_codec::base64::encode_standard(&bytes)
}

pub fn read_http_headers(stream: &mut impl Read) -> Result<Vec<u8>, WsError> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 256];
    loop {
        let n = stream
            .read(&mut tmp)
            .map_err(|e| WsError::Io(e.to_string()))?;
        if n == 0 {
            return Err(WsError::Handshake("connection closed".into()));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return Ok(buf);
        }
        if buf.len() > 8192 {
            return Err(WsError::Handshake("headers too large".into()));
        }
    }
}

pub fn client_handshake(
    stream: &mut (impl Read + Write),
    host: &str,
    path: &str,
) -> Result<String, WsError> {
    let key = generate_key();
    let req = client_request(host, path, &key);
    stream
        .write_all(req.as_bytes())
        .map_err(|e| WsError::Io(e.to_string()))?;
    let buf = read_http_headers(stream)?;
    validate_server_response(&buf, &key)?;
    Ok(key)
}

fn validate_server_response(buf: &[u8], key: &str) -> Result<(), WsError> {
    let text = std::str::from_utf8(buf).map_err(|_| WsError::Handshake("invalid utf8".into()))?;
    if !text.starts_with("HTTP/1.1 101") {
        return Err(WsError::Handshake(format!(
            "expected 101, got {}",
            text.lines().next().unwrap_or("")
        )));
    }
    let expected = ws_accept_key(key);
    let mut accept = None;
    for line in text.split("\r\n").skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("Sec-WebSocket-Accept") {
                accept = Some(value.trim().to_string());
            }
        }
    }
    match accept {
        Some(a) if a == expected => Ok(()),
        Some(a) => Err(WsError::Handshake(format!("bad accept: {a}"))),
        None => Err(WsError::Handshake("missing Sec-WebSocket-Accept".into())),
    }
}

pub fn server_handshake(stream: &mut (impl Read + Write)) -> Result<(), WsError> {
    let buf = read_http_headers(stream)?;
    let (head, _) = parse_request(&buf).map_err(|e| WsError::Handshake(format!("{e:?}")))?;
    let key = head
        .headers
        .get("sec-websocket-key")
        .ok_or_else(|| WsError::Handshake("missing Sec-WebSocket-Key".into()))?;
    let version = head
        .headers
        .get("sec-websocket-version")
        .ok_or_else(|| WsError::Handshake("missing Sec-WebSocket-Version".into()))?;
    if version != "13" {
        return Err(WsError::Handshake(format!("bad version {version}")));
    }
    let accept = ws_accept_key(key);
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream
        .write_all(resp.as_bytes())
        .map_err(|e| WsError::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_vector() {
        assert_eq!(
            ws_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }
}
