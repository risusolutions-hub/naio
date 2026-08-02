//! OS-backed random bytes with PRNG fallback.

use niao_rand::fill_os_random;

pub fn fill_random(buf: &mut [u8]) {
    fill_os_random(buf);
}
