//! Message framing: NDJSON and LSP-style Content-Length headers.

use crate::codec::{decode, encode, MAX_BYTES};
use crate::error::EngineError;
use crate::message::Message;

/// Framing style for stream transports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStyle {
    /// One JSON message per line (trailing `\n`).
    Ndjson,
    /// `Content-Length: N\r\n\r\n` + body (Language Server Protocol style).
    ContentLength,
}

impl FrameStyle {
    pub fn parse(name: &str) -> Result<Self, EngineError> {
        match name {
            "ndjson" | "nl" | "newline" => Ok(FrameStyle::Ndjson),
            "content-length" | "lsp" | "cl" => Ok(FrameStyle::ContentLength),
            other => Err(EngineError::Invalid(format!(
                "unknown frame style '{other}' (expected ndjson or content-length)"
            ))),
        }
    }
}

impl Default for FrameStyle {
    fn default() -> Self {
        FrameStyle::Ndjson
    }
}

/// Encode a message with framing bytes ready to write to a stream.
pub fn frame(msg: &Message, style: FrameStyle) -> String {
    let body = encode(msg);
    match style {
        FrameStyle::Ndjson => {
            let mut out = body;
            out.push('\n');
            out
        }
        FrameStyle::ContentLength => {
            let len = body.len();
            format!("Content-Length: {len}\r\n\r\n{body}")
        }
    }
}

/// Frame raw JSON text the same way.
pub fn frame_text(body: &str, style: FrameStyle) -> String {
    match style {
        FrameStyle::Ndjson => {
            let mut out = body.to_string();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out
        }
        FrameStyle::ContentLength => {
            format!("Content-Length: {}\r\n\r\n{body}", body.len())
        }
    }
}

/// Result of pulling zero or more complete frames from a buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct UnframeResult {
    pub messages: Vec<Message>,
    /// Remaining incomplete bytes (UTF-8 text).
    pub rest: String,
}

/// Extract complete framed messages from a growable text buffer.
pub fn unframe(buffer: &str, style: FrameStyle) -> Result<UnframeResult, EngineError> {
    match style {
        FrameStyle::Ndjson => unframe_ndjson(buffer),
        FrameStyle::ContentLength => unframe_content_length(buffer),
    }
}

fn unframe_ndjson(buffer: &str) -> Result<UnframeResult, EngineError> {
    let mut messages = Vec::new();
    let mut start = 0usize;
    let bytes = buffer.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'\n' {
            let line = buffer[start..i].trim_end_matches('\r');
            start = i + 1;
            if line.is_empty() {
                continue;
            }
            if line.len() > MAX_BYTES {
                return Err(EngineError::Limit(format!(
                    "framed message exceeds {MAX_BYTES} bytes"
                )));
            }
            messages.push(decode(line)?);
        }
    }
    Ok(UnframeResult {
        messages,
        rest: buffer[start..].to_string(),
    })
}

fn unframe_content_length(buffer: &str) -> Result<UnframeResult, EngineError> {
    let mut messages = Vec::new();
    let mut rest = buffer;
    loop {
        let header_end = match rest.find("\r\n\r\n") {
            Some(i) => i,
            None => break,
        };
        let headers = &rest[..header_end];
        let mut content_length: Option<usize> = None;
        for line in headers.split("\r\n") {
            let lower = line.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                let n: usize = v
                    .trim()
                    .parse()
                    .map_err(|_| EngineError::Framing("invalid Content-Length header".into()))?;
                content_length = Some(n);
            }
        }
        let len = content_length
            .ok_or_else(|| EngineError::Framing("missing Content-Length header".into()))?;
        if len > MAX_BYTES {
            return Err(EngineError::Limit(format!(
                "framed message exceeds {MAX_BYTES} bytes"
            )));
        }
        let body_start = header_end + 4;
        if rest.len() < body_start + len {
            break;
        }
        let body = &rest[body_start..body_start + len];
        messages.push(decode(body)?);
        rest = &rest[body_start + len..];
    }
    Ok(UnframeResult {
        messages,
        rest: rest.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Id, Request};

    #[test]
    fn ndjson_roundtrip() {
        let msg = Message::Request(Request::call("a", None, Id::Number(1)));
        let framed = frame(&msg, FrameStyle::Ndjson);
        let u = unframe(&framed, FrameStyle::Ndjson).unwrap();
        assert_eq!(u.messages.len(), 1);
        assert!(u.rest.is_empty());
    }

    #[test]
    fn content_length_partial() {
        let msg = Message::Request(Request::notify("n", None));
        let framed = frame(&msg, FrameStyle::ContentLength);
        let partial = &framed[..framed.len() / 2];
        let u = unframe(partial, FrameStyle::ContentLength).unwrap();
        assert!(u.messages.is_empty());
        assert!(!u.rest.is_empty());
        let full = unframe(
            &(u.rest + &framed[framed.len() / 2..]),
            FrameStyle::ContentLength,
        )
        .unwrap();
        // Rebuild: rest + remaining
        let _ = full;
        let u2 = unframe(&framed, FrameStyle::ContentLength).unwrap();
        assert_eq!(u2.messages.len(), 1);
    }
}
