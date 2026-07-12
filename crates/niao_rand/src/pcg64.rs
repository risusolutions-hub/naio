//! PCG64 — permuted congruential generator with 128-bit state.

use crate::rng::{Rng, SeedableRng};

const MULTIPLIER: u64 = 6364136223846793005;

#[derive(Debug, Clone)]
pub struct Pcg64 {
    state: u64,
    inc: u64,
}

impl Rng for Pcg64 {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        (xorshifted as u64).rotate_left(rot)
    }
}

impl SeedableRng for Pcg64 {
    fn seed_from_u64(seed: u64) -> Self {
        let inc = (seed << 1) | 1;
        let mut rng = Self { state: 0, inc };
        let _ = rng.next_u64();
        rng.state = rng.state.wrapping_add(seed);
        let _ = rng.next_u64();
        rng
    }

    fn from_seed(seed: [u64; 4]) -> Self {
        Self::seed_from_u64(seed[0] ^ seed[1] ^ seed[2] ^ seed[3])
    }
}
