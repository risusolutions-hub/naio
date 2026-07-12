//! UUID v4 (random) and v7 (timestamp-ordered).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uuid {
    bytes: [u8; 16],
}

#[derive(Debug, PartialEq, Eq)]
pub enum UuidError {
    InvalidLength,
    InvalidChar,
    InvalidFormat,
}

impl fmt::Display for UuidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => write!(f, "uuid string must be 36 characters"),
            Self::InvalidChar => write!(f, "invalid uuid character"),
            Self::InvalidFormat => write!(f, "invalid uuid format"),
        }
    }
}

impl std::error::Error for UuidError {}

impl Uuid {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    /// Random UUID v4 using OS entropy (with xorshift fallback).
    pub fn new_v4() -> Self {
        let mut bytes = [0u8; 16];
        fill_random(&mut bytes);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self { bytes }
    }

    /// Timestamp-ordered UUID v7 (RFC 9562).
    pub fn new_v7() -> Self {
        let ts_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut bytes = [0u8; 16];
        bytes[0..6].copy_from_slice(&ts_ms.to_be_bytes()[2..8]);
        fill_random(&mut bytes[6..]);
        bytes[6] = (bytes[6] & 0x0f) | 0x70;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self { bytes }
    }

    pub fn parse(s: &str) -> Result<Self, UuidError> {
        if s.len() != 36 {
            return Err(UuidError::InvalidLength);
        }
        let b = s.as_bytes();
        for (i, &c) in b.iter().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                if c != b'-' {
                    return Err(UuidError::InvalidFormat);
                }
            } else if !c.is_ascii_hexdigit() {
                return Err(UuidError::InvalidChar);
            }
        }
        let mut out = [0u8; 16];
        let mut oi = 0;
        let mut i = 0;
        while i < 36 {
            if b[i] == b'-' {
                i += 1;
                continue;
            }
            let hi = hex_nibble(b[i]).ok_or(UuidError::InvalidChar)?;
            let lo = hex_nibble(b[i + 1]).ok_or(UuidError::InvalidChar)?;
            out[oi] = (hi << 4) | lo;
            oi += 1;
            i += 2;
        }
        Ok(Self { bytes: out })
    }

    pub fn version(&self) -> u8 {
        (self.bytes[6] >> 4) & 0x0f
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.bytes[0],
            self.bytes[1],
            self.bytes[2],
            self.bytes[3],
            self.bytes[4],
            self.bytes[5],
            self.bytes[6],
            self.bytes[7],
            self.bytes[8],
            self.bytes[9],
            self.bytes[10],
            self.bytes[11],
            self.bytes[12],
            self.bytes[13],
            self.bytes[14],
            self.bytes[15]
        )
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uuid({self})")
    }
}

#[inline]
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

static XORSIFT_STATE: AtomicU64 = AtomicU64::new(0);

fn fill_random(buf: &mut [u8]) {
    if try_os_random(buf) {
        return;
    }
    let mut state = XORSIFT_STATE.load(Ordering::Relaxed);
    if state == 0 {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            ^ {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                std::thread::current().id().hash(&mut h);
                h.finish()
            };
        state = seed | 1;
    }
    for chunk in buf.chunks_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_le_bytes();
        for (dst, src) in chunk.iter_mut().zip(bytes.iter()) {
            *dst = *src;
        }
    }
    XORSIFT_STATE.store(state, Ordering::Relaxed);
}

#[cfg(unix)]
fn try_os_random(buf: &mut [u8]) -> bool {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .is_ok()
}

#[cfg(windows)]
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptGenRandom(
        h_algorithm: *mut std::ffi::c_void,
        pb_buffer: *mut u8,
        cb_buffer: u32,
        dw_flags: u32,
    ) -> i32;
}

#[cfg(windows)]
fn try_os_random(buf: &mut [u8]) -> bool {
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        ) == 0
    }
}

#[cfg(not(any(unix, windows)))]
fn try_os_random(_buf: &mut [u8]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn v4_format_and_uniqueness() {
        let mut seen = HashSet::new();
        for _ in 0..256 {
            let u = Uuid::new_v4();
            assert_eq!(u.version(), 4);
            let s = u.to_string();
            assert_eq!(s.len(), 36);
            assert_eq!(&s[14..15], "4");
            assert!(matches!(s.as_bytes()[19], b'8' | b'9' | b'a' | b'b' | b'A' | b'B'));
            assert!(seen.insert(s));
        }
    }

    #[test]
    fn v7_monotonic_and_version() {
        let a = Uuid::new_v7();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = Uuid::new_v7();
        assert_eq!(a.version(), 7);
        assert_eq!(b.version(), 7);
        assert!(b.to_string() > a.to_string() || b.to_string() >= a.to_string());
    }

    #[test]
    fn parse_roundtrip() {
        let u = Uuid::new_v4();
        let s = u.to_string();
        let p = Uuid::parse(&s).unwrap();
        assert_eq!(u, p);
    }
}
