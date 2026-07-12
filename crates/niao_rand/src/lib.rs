//! Zero-dependency random number generation for Niao.
//!
//! Provides PCG64 and xoshiro256** PRNGs, OS entropy seeding, unbiased integer
//! ranges (Lemire), uniform floats, and slice shuffle/choose.

mod entropy;
mod pcg64;
mod rng;
mod seq;
mod xoshiro256;

pub use entropy::{fill_os_random, seed256};
pub use pcg64::Pcg64;
pub use rng::{Rng, SeedableRng};
pub use seq::SliceRandom;
pub use xoshiro256::{StdRng, Xoshiro256StarStar};

/// Thread-local default generator seeded from OS entropy on first use.
pub fn thread_rng() -> ThreadRng {
    ThreadRng
}

/// Marker for lazily initialized thread-local RNG.
pub struct ThreadRng;

impl Rng for ThreadRng {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        thread_local! {
            static RNG: std::cell::RefCell<Xoshiro256StarStar> = {
                std::cell::RefCell::new(Xoshiro256StarStar::from_seed(seed256()))
            };
        }
        RNG.with(|cell| cell.borrow_mut().next_u64())
    }
}

#[cfg(test)]
mod tests;
