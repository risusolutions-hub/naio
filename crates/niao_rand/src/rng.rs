//! Core RNG trait, Lemire unbiased range sampling, and float helpers.

use std::ops::Range;

/// Random number generator interface.
pub trait Rng: Sized {
    fn next_u64(&mut self) -> u64;

    #[inline]
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform `u64` in `[min, max)` using Lemire's method (no modulo bias).
    #[inline]
    fn gen_range(&mut self, range: Range<u64>) -> u64 {
        let start = range.start;
        let span = range.end.wrapping_sub(start);
        if span == 0 {
            panic!("empty range");
        }
        start.wrapping_add(lemire_u64(self, span))
    }

    /// Uniform `i64` in `[min, max)`.
    #[inline]
    fn gen_range_i64(&mut self, min: i64, max: i64) -> i64 {
        assert!(min < max, "empty i64 range");
        let span = (max - min) as u64;
        min + lemire_u64(self, span) as i64
    }

    /// Uniform `usize` in `[min, max)`.
    #[inline]
    fn gen_range_usize(&mut self, min: usize, max: usize) -> usize {
        assert!(min < max, "empty usize range");
        min + lemire_u64(self, (max - min) as u64) as usize
    }

    #[inline]
    fn gen_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Uniform `f64` in `[0.0, 1.0)`.
    #[inline]
    fn gen_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
        (self.next_u64() >> 11) as f64 * SCALE
    }

    /// Uniform `f32` in `[0.0, 1.0)`.
    #[inline]
    fn gen_f32(&mut self) -> f32 {
        const SCALE: f32 = 1.0 / (1u32 << 24) as f32;
        (self.next_u32() >> 8) as f32 * SCALE
    }

    #[inline]
    fn fill_bytes(&mut self, buf: &mut [u8]) {
        let mut chunks = buf.chunks_mut(8);
        while let Some(chunk) = chunks.next() {
            let val = self.next_u64().to_le_bytes();
            for (dst, src) in chunk.iter_mut().zip(val.iter()) {
                *dst = *src;
            }
        }
    }
}

/// Seedable PRNG constructors.
pub trait SeedableRng: Rng + Sized {
    fn seed_from_u64(seed: u64) -> Self;
    fn from_seed(seed: [u64; 4]) -> Self;
}

#[inline]
fn lemire_u64(rng: &mut impl Rng, span: u64) -> u64 {
    debug_assert!(span > 0);
    let threshold = span.wrapping_neg() % span;
    loop {
        let r = rng.next_u64();
        let m = (r as u128).wrapping_mul(span as u128);
        let lo = m as u64;
        if lo >= threshold {
            return (m >> 64) as u64;
        }
    }
}
