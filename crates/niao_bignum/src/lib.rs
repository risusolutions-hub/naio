//! Zero-dependency arbitrary-precision integers for Niao.
//!
//! Sign + magnitude `Vec<u64>` limbs, schoolbook multiply with Karatsuba above
//! 256 bits, Knuth long division, and decimal parse/format.

mod bigint;
mod sign;
pub(crate) mod uint;

#[doc(hidden)]
pub mod bench_util {
    use super::uint::BigUint;

    pub fn bench_schoolbook_vs_karatsuba(limbs: usize) -> (f64, f64) {
        let mut seed_a = BigUint::from_u64(0xDEAD_BEEF_CAFE_BABEu64);
        let mut seed_b = BigUint::from_u64(0x0123_4567_89AB_CDEFu64);
        for _ in 1..limbs {
            seed_a = seed_a
                .mul_limb(0x100000001B3)
                .add_mag(&BigUint::from_u64(0x27));
            seed_b = seed_b
                .mul_limb(0x100000001B3)
                .add_mag(&BigUint::from_u64(0x42));
        }
        let school = {
            let start = std::time::Instant::now();
            for _ in 0..256 {
                std::hint::black_box(seed_a.mul_schoolbook(&seed_b));
            }
            256.0 / start.elapsed().as_secs_f64()
        };
        let kara = {
            let start = std::time::Instant::now();
            for _ in 0..256 {
                std::hint::black_box(seed_a.mul_karatsuba(&seed_b));
            }
            256.0 / start.elapsed().as_secs_f64()
        };
        (school, kara)
    }
}

pub use bigint::{BigInt, ParseBigIntError};
pub use sign::Sign;

/// Limb count threshold before Karatsuba multiply kicks in (256 bits).
pub const KARATSUBA_THRESHOLD: usize = uint::KARATSUBA_THRESHOLD;

#[cfg(test)]
mod prop_tests;
