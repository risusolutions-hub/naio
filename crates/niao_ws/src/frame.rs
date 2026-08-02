//! RFC 6455 WebSocket frame codec.

use crate::error::WsError;
use crate::role::Role;

pub const OPCODE_CONT: u8 = 0x0;
pub const OPCODE_TEXT: u8 = 0x1;
pub const OPCODE_BINARY: u8 = 0x2;
pub const OPCODE_CLOSE: u8 = 0x8;
pub const OPCODE_PING: u8 = 0x9;
pub const OPCODE_PONG: u8 = 0xA;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub fin: bool,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn text(payload: impl Into<Vec<u8>>, fin: bool) -> Self {
        Self {
            fin,
            opcode: OPCODE_TEXT,
            payload: payload.into(),
        }
    }

    pub fn binary(payload: impl Into<Vec<u8>>, fin: bool) -> Self {
        Self {
            fin,
            opcode: OPCODE_BINARY,
            payload: payload.into(),
        }
    }

    pub fn ping(payload: Vec<u8>) -> Self {
        Self {
            fin: true,
            opcode: OPCODE_PING,
            payload,
        }
    }

    pub fn pong(payload: Vec<u8>) -> Self {
        Self {
            fin: true,
            opcode: OPCODE_PONG,
            payload,
        }
    }

    pub fn close(code: Option<u16>, reason: &str) -> Self {
        let mut payload = Vec::new();
        if let Some(c) = code {
            payload.extend_from_slice(&c.to_be_bytes());
            payload.extend_from_slice(reason.as_bytes());
        }
        Self {
            fin: true,
            opcode: OPCODE_CLOSE,
            payload,
        }
    }
}

pub fn encode_frame(frame: &Frame, role: Role, out: &mut Vec<u8>) {
    let mut b0 = frame.opcode & 0x0F;
    if frame.fin {
        b0 |= 0x80;
    }
    out.push(b0);
    let len = frame.payload.len();
    let mask = role == Role::Client;
    if len < 126 {
        out.push((len as u8) | if mask { 0x80 } else { 0 });
    } else if len <= 0xFFFF {
        out.push(126 | if mask { 0x80 } else { 0 });
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(127 | if mask { 0x80 } else { 0 });
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    let mask_key = if mask {
        let key = random_mask();
        out.extend_from_slice(&key);
        Some(key)
    } else {
        None
    };
    if let Some(key) = mask_key {
        for (i, b) in frame.payload.iter().enumerate() {
            out.push(b ^ key[i % 4]);
        }
    } else {
        out.extend_from_slice(&frame.payload);
    }
}

fn random_mask() -> [u8; 4] {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let v = t ^ t.rotate_left(17) ^ 0x9E3779B97F4A7C15;
    [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8]
}

pub fn decode_frame(buf: &[u8], role: Role) -> Result<(Frame, usize), WsError> {
    if buf.len() < 2 {
        return Err(WsError::Incomplete);
    }
    let b0 = buf[0];
    let b1 = buf[1];
    if b0 & 0x70 != 0 {
        return Err(WsError::Protocol("RSV bits must be 0".into()));
    }
    let fin = b0 & 0x80 != 0;
    let opcode = b0 & 0x0F;
    let masked = b1 & 0x80 != 0;
    if role == Role::Server && !masked {
        return Err(WsError::Protocol("client frame must be masked".into()));
    }
    if role == Role::Client && masked {
        return Err(WsError::Protocol("server frame must not be masked".into()));
    }
    let mut pos = 2usize;
    let mut len = (b1 & 0x7F) as u64;
    match len {
        126 => {
            if buf.len() < pos + 2 {
                return Err(WsError::Incomplete);
            }
            len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as u64;
            pos += 2;
        }
        127 => {
            if buf.len() < pos + 8 {
                return Err(WsError::Incomplete);
            }
            len = u64::from_be_bytes([
                buf[pos],
                buf[pos + 1],
                buf[pos + 2],
                buf[pos + 3],
                buf[pos + 4],
                buf[pos + 5],
                buf[pos + 6],
                buf[pos + 7],
            ]);
            pos += 8;
        }
        _ => {}
    }
    let mask_key = if masked {
        if buf.len() < pos + 4 {
            return Err(WsError::Incomplete);
        }
        let key = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
        pos += 4;
        Some(key)
    } else {
        None
    };
    if buf.len() < pos + len as usize {
        return Err(WsError::Incomplete);
    }
    let mut payload = buf[pos..pos + len as usize].to_vec();
    pos += len as usize;
    if let Some(key) = mask_key {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= key[i % 4];
        }
    }
    Ok((
        Frame {
            fin,
            opcode,
            payload,
        },
        pos,
    ))
}

pub fn parse_close_payload(payload: &[u8]) -> Result<(Option<u16>, String), WsError> {
    if payload.is_empty() {
        return Ok((None, String::new()));
    }
    if payload.len() == 1 {
        return Err(WsError::Protocol("close payload too short".into()));
    }
    let code = u16::from_be_bytes([payload[0], payload[1]]);
    validate_close_code(code)?;
    let reason = if payload.len() > 2 {
        if !crate::utf8::is_valid_utf8(&payload[2..]) {
            return Err(WsError::Protocol("close reason not utf8".into()));
        }
        String::from_utf8_lossy(&payload[2..]).into_owned()
    } else {
        String::new()
    };
    Ok((Some(code), reason))
}

pub fn validate_close_code(code: u16) -> Result<(), WsError> {
    if code < 1000 {
        return Err(WsError::Protocol(format!("invalid close code {code}")));
    }
    if (1004..1007).contains(&code) || code == 1012 || (1014..=1015).contains(&code) {
        return Err(WsError::Protocol(format!("reserved close code {code}")));
    }
    if code >= 5000 {
        return Err(WsError::Protocol(format!("invalid close code {code}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_client_text() {
        let frame = Frame::text(b"hello".to_vec(), true);
        let mut out = Vec::new();
        encode_frame(&frame, Role::Client, &mut out);
        let (dec, n) = decode_frame(&out, Role::Server).unwrap();
        assert_eq!(n, out.len());
        assert_eq!(dec.payload, b"hello");
        assert!(dec.fin);
    }

    #[test]
    fn server_rejects_unmasked_client() {
        let frame = Frame::text(b"x".to_vec(), true);
        let mut out = Vec::new();
        encode_frame(&frame, Role::Server, &mut out);
        assert!(matches!(
            decode_frame(&out, Role::Server),
            Err(WsError::Protocol(_))
        ));
    }

    #[test]
    fn extended_16bit_length() {
        let payload = vec![0u8; 200];
        let frame = Frame::binary(payload.clone(), true);
        let mut out = Vec::new();
        encode_frame(&frame, Role::Server, &mut out);
        let (dec, _) = decode_frame(&out, Role::Client).unwrap();
        assert_eq!(dec.payload.len(), 200);
    }

    #[test]
    fn invalid_close_code_rejected() {
        assert!(validate_close_code(1002).is_ok());
        assert!(validate_close_code(999).is_err());
        assert!(validate_close_code(1005).is_err());
    }
}
