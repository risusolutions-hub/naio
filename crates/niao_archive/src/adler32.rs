//! Adler-32 (zlib).

const MOD: u32 = 65521;

#[inline]
pub fn adler32(data: &[u8], init: u32) -> u32 {
    let mut a = init & 0xFFFF;
    let mut b = (init >> 16) & 0xFFFF;
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}
