//! UUID helpers extending [`niao_codec::Uuid`] with v6 and metadata extraction.

use crate::entropy::fill_random;
use niao_codec::Uuid;

pub use niao_codec::UuidError;
use std::time::{SystemTime, UNIX_EPOCH};

/// UUID epoch offset: 100-ns intervals between UUID epoch and Unix epoch.
const UUID_UNIX_OFFSET: u64 = 0x01B2_1DD2_1381_4000;

/// Random UUID v4 (delegates to codec).
#[inline]
pub fn uuid4() -> Uuid {
    Uuid::new_v4()
}

/// Timestamp-ordered UUID v7 (delegates to codec).
#[inline]
pub fn uuid7() -> Uuid {
    Uuid::new_v7()
}

/// UUID v6 — time-ordered per RFC 9562 (reordered v1-style fields).
pub fn uuid6() -> Uuid {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    uuid6_from_timestamp(ts_ms)
}

/// Build UUID v6 from Unix milliseconds.
pub fn uuid6_from_timestamp(ts_ms: u64) -> Uuid {
    let ts100 = ts_ms
        .saturating_mul(10_000)
        .saturating_add(UUID_UNIX_OFFSET);
    let time_high = (ts100 >> 28) as u32;
    let time_mid = ((ts100 >> 12) & 0xffff) as u16;
    let time_low = (ts100 & 0x0fff) as u16;
    let time_low_and_ver = 0x6000u16 | time_low;

    let mut clock_seq = [0u8; 2];
    fill_random(&mut clock_seq);
    clock_seq[0] = (clock_seq[0] & 0x3f) | 0x80;

    let mut node = [0u8; 6];
    fill_random(&mut node);
    node[0] |= 0x01;

    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&time_high.to_be_bytes());
    bytes[4..6].copy_from_slice(&time_mid.to_be_bytes());
    bytes[6..8].copy_from_slice(&time_low_and_ver.to_be_bytes());
    bytes[8] = clock_seq[0];
    bytes[9] = clock_seq[1];
    bytes[10..16].copy_from_slice(&node);

    Uuid::from_bytes(bytes)
}

#[inline]
pub fn parse(s: &str) -> Result<Uuid, UuidError> {
    Uuid::parse(s)
}

#[inline]
pub fn is_valid(s: &str) -> bool {
    Uuid::parse(s).is_ok()
}

/// Extract Unix milliseconds for time-based UUID versions (v1/v6/v7); `None` for others.
pub fn timestamp_ms(uuid: &Uuid) -> Option<u64> {
    match uuid.version() {
        7 => {
            let b = uuid.as_bytes();
            let mut ts = [0u8; 8];
            ts[0..2].fill(0);
            ts[2..8].copy_from_slice(&b[0..6]);
            Some(u64::from_be_bytes(ts))
        }
        6 => {
            let b = uuid.as_bytes();
            let time_high = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
            let time_mid = u16::from_be_bytes([b[4], b[5]]);
            let time_low = u16::from_be_bytes([b[6], b[7]]) & 0x0fff;
            let ts100 = ((time_high as u64) << 28) | ((time_mid as u64) << 12) | (time_low as u64);
            let unix100 = ts100.saturating_sub(UUID_UNIX_OFFSET);
            Some(unix100 / 10_000)
        }
        _ => None,
    }
}

pub fn from_bytes(bytes: &[u8]) -> Result<Uuid, UuidError> {
    if bytes.len() != 16 {
        return Err(UuidError::InvalidLength);
    }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(bytes);
    Ok(Uuid::from_bytes(arr))
}

pub fn to_bytes(uuid: &Uuid) -> [u8; 16] {
    *uuid.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v6_version_and_parse() {
        let u = uuid6();
        assert_eq!(u.version(), 6);
        let s = u.to_string();
        let p = parse(&s).unwrap();
        assert_eq!(u, p);
        assert!(timestamp_ms(&u).is_some());
    }

    #[test]
    fn v7_timestamp() {
        let u = uuid7();
        assert_eq!(u.version(), 7);
        assert!(timestamp_ms(&u).is_some());
    }
}
