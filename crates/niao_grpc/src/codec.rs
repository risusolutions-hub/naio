//! Length-prefixed gRPC message framing (uncompressed).

use crate::error::{GrpcError, GrpcResult};
use bytes::{Buf, BufMut, Bytes, BytesMut};

const MAX_MESSAGE_LEN: u32 = 64 * 1024 * 1024; // 64 MiB soft cap

/// Encode one gRPC data frame: 1 compressed-flag byte + 4-byte BE length + payload.
pub fn frame_message(payload: &[u8]) -> GrpcResult<Bytes> {
    if payload.len() as u64 > MAX_MESSAGE_LEN as u64 {
        return Err(GrpcError::new(format!(
            "message too large: {} bytes (max {MAX_MESSAGE_LEN})",
            payload.len()
        )));
    }
    let mut buf = BytesMut::with_capacity(5 + payload.len());
    buf.put_u8(0); // uncompressed
    buf.put_u32(payload.len() as u32);
    buf.extend_from_slice(payload);
    Ok(buf.freeze())
}

/// Decode the first complete framed message; returns (payload, bytes_consumed).
pub fn unframe_one(data: &[u8]) -> GrpcResult<(Vec<u8>, usize)> {
    if data.len() < 5 {
        return Err(GrpcError::new(format!(
            "truncated gRPC frame header ({} bytes)",
            data.len()
        )));
    }
    let compressed = data[0];
    if compressed != 0 {
        return Err(GrpcError::new(
            "compressed gRPC messages are not supported in ngrpc 0.1",
        ));
    }
    let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
    if len > MAX_MESSAGE_LEN {
        return Err(GrpcError::new(format!(
            "message length {len} exceeds max {MAX_MESSAGE_LEN}"
        )));
    }
    let total = 5 + len as usize;
    if data.len() < total {
        return Err(GrpcError::new(format!(
            "truncated gRPC frame body (need {total}, have {})",
            data.len()
        )));
    }
    Ok((data[5..total].to_vec(), total))
}

/// Decode every complete framed message in `data`.
pub fn unframe_all(data: &[u8]) -> GrpcResult<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let (msg, n) = unframe_one(&data[offset..])?;
        out.push(msg);
        offset += n;
    }
    Ok(out)
}

/// Streaming frame decoder that can accept partial reads.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: BytesMut,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// Pop the next complete message, if available.
    pub fn next_message(&mut self) -> GrpcResult<Option<Vec<u8>>> {
        if self.buf.len() < 5 {
            return Ok(None);
        }
        let compressed = self.buf[0];
        if compressed != 0 {
            return Err(GrpcError::new(
                "compressed gRPC messages are not supported in ngrpc 0.1",
            ));
        }
        let len = u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]);
        if len > MAX_MESSAGE_LEN {
            return Err(GrpcError::new(format!(
                "message length {len} exceeds max {MAX_MESSAGE_LEN}"
            )));
        }
        let total = 5 + len as usize;
        if self.buf.len() < total {
            return Ok(None);
        }
        let _ = self.buf.split_to(5);
        let payload = self.buf.split_to(len as usize).to_vec();
        Ok(Some(payload))
    }

    pub fn remaining(&self) -> usize {
        self.buf.remaining()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let payload = b"hello-grpc";
        let framed = frame_message(payload).unwrap();
        assert_eq!(framed[0], 0);
        let (decoded, n) = unframe_one(&framed).unwrap();
        assert_eq!(n, framed.len());
        assert_eq!(decoded, payload);
    }

    #[test]
    fn unframe_all_two() {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&frame_message(b"a").unwrap());
        buf.extend_from_slice(&frame_message(b"bb").unwrap());
        let msgs = unframe_all(&buf).unwrap();
        assert_eq!(msgs, vec![b"a".to_vec(), b"bb".to_vec()]);
    }

    #[test]
    fn decoder_partial() {
        let framed = frame_message(b"xyz").unwrap();
        let mut dec = FrameDecoder::new();
        dec.push(&framed[..3]);
        assert!(dec.next_message().unwrap().is_none());
        dec.push(&framed[3..]);
        assert_eq!(dec.next_message().unwrap().unwrap(), b"xyz");
    }

    #[test]
    fn empty_payload() {
        let framed = frame_message(b"").unwrap();
        assert_eq!(framed.len(), 5);
        let (p, _) = unframe_one(&framed).unwrap();
        assert!(p.is_empty());
    }
}
