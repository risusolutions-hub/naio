//! RFC 1952 gzip wrapper.

use crate::crc32;
use crate::deflate::inflate;
use crate::error::{Error, Result};

pub fn decode(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() < 18 || input[0] != 0x1f || input[1] != 0x8b {
        return Err(Error::Message("not gzip".into()));
    }
    let method = input[2];
    if method != 8 {
        return Err(Error::Unsupported(format!("gzip method {method}")));
    }
    let flags = input[3];
    let mut pos = 10usize;
    if flags & 0x04 != 0 {
        if pos + 2 > input.len() {
            return Err(Error::Truncated);
        }
        let xlen = u16::from_le_bytes([input[pos], input[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    if flags & 0x08 != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flags & 0x10 != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flags & 0x02 != 0 {
        pos += 2;
    }
    if pos + 8 > input.len() {
        return Err(Error::Truncated);
    }
    let payload = &input[pos..input.len() - 8];
    let out = inflate(payload)?;
    let crc = u32::from_le_bytes(input[input.len() - 8..input.len() - 4].try_into().unwrap());
    let isize = u32::from_le_bytes(input[input.len() - 4..].try_into().unwrap());
    if crc32::crc32(&out) != crc {
        return Err(Error::CrcMismatch);
    }
    if isize as usize != out.len() {
        return Err(Error::Message("gzip isize mismatch".into()));
    }
    Ok(out)
}

pub fn encode(input: &[u8]) -> Result<Vec<u8>> {
    crate::deflate::gzip_encode(input)
}
