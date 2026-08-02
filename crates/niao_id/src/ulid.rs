//! ULID — 128-bit lexicographically sortable identifiers (Crockford base32).

use crate::entropy::fill_random;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

const ENCODING: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const DECODE: [i8; 256] = {
    let mut table = [-1i8; 256];
    let mut i = 0usize;
    while i < 32 {
        table[ENCODING[i] as usize] = i as i8;
        i += 1;
    }
    table[b'-' as usize] = -2; // ignore
    table
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ulid {
    bytes: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UlidError {
    InvalidLength,
    InvalidChar,
    TimestampOverflow,
}

impl fmt::Display for UlidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => write!(f, "ulid must be 26 characters"),
            Self::InvalidChar => write!(f, "invalid ulid character"),
            Self::TimestampOverflow => write!(f, "timestamp out of range"),
        }
    }
}

impl std::error::Error for UlidError {}

impl Ulid {
    /// New ULID with current wall-clock milliseconds and 80 random bits.
    pub fn new() -> Self {
        let ts = now_ms();
        let mut rand = [0u8; 10];
        fill_random(&mut rand);
        Self::from_parts(ts, &rand)
    }

    /// Construct from 48-bit millisecond timestamp and 80-bit randomness.
    pub fn from_parts(timestamp_ms: u64, randomness: &[u8; 10]) -> Self {
        let mut bytes = [0u8; 16];
        bytes[0..6].copy_from_slice(&timestamp_ms.to_be_bytes()[2..8]);
        bytes[6..16].copy_from_slice(randomness);
        Self { bytes }
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    pub fn timestamp_ms(&self) -> u64 {
        let mut ts = [0u8; 8];
        ts[2..8].copy_from_slice(&self.bytes[0..6]);
        u64::from_be_bytes(ts)
    }

    pub fn parse(s: &str) -> Result<Self, UlidError> {
        if s.len() != 26 {
            return Err(UlidError::InvalidLength);
        }
        let mut out = [0u8; 16];
        let mut idx = 0usize;
        let mut bit_buf = 0u64;
        let mut bit_len = 0u32;
        for &c in s.as_bytes() {
            let v = DECODE[c as usize];
            if v == -2 {
                continue;
            }
            if v < 0 {
                return Err(UlidError::InvalidChar);
            }
            bit_buf = (bit_buf << 5) | (v as u64);
            bit_len += 5;
            while bit_len >= 8 {
                bit_len -= 8;
                let byte = ((bit_buf >> bit_len) & 0xff) as u8;
                if idx < 16 {
                    out[idx] = byte;
                    idx += 1;
                }
            }
        }
        if idx != 16 {
            return Err(UlidError::InvalidLength);
        }
        Ok(Self { bytes: out })
    }

    pub fn is_valid(s: &str) -> bool {
        if s.len() != 26 {
            return false;
        }
        for &c in s.as_bytes() {
            let v = DECODE[c as usize];
            if v < 0 && v != -2 {
                return false;
            }
        }
        true
    }
}

impl fmt::Display for Ulid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = [0u8; 26];
        let mut bit_buf = 0u128;
        let mut bit_len = 0u32;
        let mut oi = 0usize;
        for &b in &self.bytes {
            bit_buf = (bit_buf << 8) | b as u128;
            bit_len += 8;
            while bit_len >= 5 && oi < 26 {
                bit_len -= 5;
                let idx = ((bit_buf >> bit_len) & 0x1f) as usize;
                out[oi] = ENCODING[idx];
                oi += 1;
            }
        }
        // flush remaining bits
        if oi < 26 {
            let idx = ((bit_buf << (5 - bit_len)) & 0x1f) as usize;
            out[oi] = ENCODING[idx];
        }
        f.write_str(std::str::from_utf8(&out).unwrap())
    }
}

impl fmt::Debug for Ulid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ulid({self})")
    }
}

/// Monotonic ULID generator — guarantees strictly increasing ULIDs within a process.
pub struct MonotonicUlid {
    last_ts: u64,
    last_rand: [u8; 10],
}

impl MonotonicUlid {
    pub fn new() -> Self {
        Self {
            last_ts: 0,
            last_rand: [0u8; 10],
        }
    }

    pub fn next(&mut self) -> Ulid {
        let mut ts = now_ms();
        if ts <= self.last_ts {
            ts = self.last_ts;
            if !increment_random(&mut self.last_rand) {
                ts = self.last_ts.saturating_add(1);
                fill_random(&mut self.last_rand);
            }
        } else {
            fill_random(&mut self.last_rand);
        }
        self.last_ts = ts;
        Ulid::from_parts(ts, &self.last_rand)
    }
}

impl Default for MonotonicUlid {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn increment_random(rand: &mut [u8; 10]) -> bool {
    for b in rand.iter_mut().rev() {
        let (next, overflow) = b.overflowing_add(1);
        *b = next;
        if !overflow {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn roundtrip_and_sortable() {
        let u = Ulid::new();
        let s = u.to_string();
        assert_eq!(s.len(), 26);
        let p = Ulid::parse(&s).unwrap();
        assert_eq!(u, p);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let v = Ulid::new();
        assert!(v.to_string() > s);
    }

    #[test]
    fn unique_batch() {
        let mut seen = HashSet::new();
        for _ in 0..512 {
            assert!(seen.insert(Ulid::new().to_string()));
        }
    }

    #[test]
    fn monotonic_increases() {
        let mut gen = MonotonicUlid::new();
        let a = gen.next().to_string();
        let b = gen.next().to_string();
        assert!(b > a);
    }
}
