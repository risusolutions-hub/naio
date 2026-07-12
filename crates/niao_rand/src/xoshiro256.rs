//! xoshiro256** — fast, statistically strong 256-bit state generator.

use crate::rng::{Rng, SeedableRng};

#[derive(Debug, Clone)]
pub struct Xoshiro256StarStar {
    s: [u64; 4],
}

impl Xoshiro256StarStar {
    #[inline]
    fn rotl(x: u64, k: u32) -> u64 {
        x.rotate_left(k)
    }
}

impl Rng for Xoshiro256StarStar {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        let result = Self::rotl(self.s[1].wrapping_mul(5), 7).wrapping_mul(9);
        let t = self.s[1] << 17;

        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = Self::rotl(self.s[3], 45);

        result
    }
}

impl SeedableRng for Xoshiro256StarStar {
    fn seed_from_u64(seed: u64) -> Self {
        let mut s = [0u64; 4];
        let mut sm = seed;
        for slot in &mut s {
            sm = sm.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = sm;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            *slot = z ^ (z >> 31);
        }
        if s.iter().all(|&v| v == 0) {
            s[0] = 1;
        }
        Self { s }
    }

    fn from_seed(seed: [u64; 4]) -> Self {
        let mut s = seed;
        if s.iter().all(|&v| v == 0) {
            s[0] = 1;
        }
        Self { s }
    }
}

/// Default fast generator for general-purpose use.
pub type StdRng = Xoshiro256StarStar;
