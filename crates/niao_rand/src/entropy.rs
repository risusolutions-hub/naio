//! OS entropy via `/dev/urandom` (Unix) or `BCryptGenRandom` (Windows).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static FALLBACK_STATE: AtomicU64 = AtomicU64::new(0);

/// Fill `buf` with cryptographically suitable random bytes when possible.
pub fn fill_os_random(buf: &mut [u8]) {
    if try_os_random(buf) {
        return;
    }
    fill_fallback(buf);
}

/// Seed a 256-bit state from OS entropy (or fallback).
pub fn seed256() -> [u64; 4] {
    let mut bytes = [0u8; 32];
    fill_os_random(&mut bytes);
    let mut out = [0u64; 4];
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        out[i] = u64::from_le_bytes(chunk.try_into().unwrap());
    }
    out
}

fn fill_fallback(buf: &mut [u8]) {
    let mut state = FALLBACK_STATE.load(Ordering::Relaxed);
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
    FALLBACK_STATE.store(state, Ordering::Relaxed);
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
