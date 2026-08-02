//! `niao_persistent` — immutable persistent collections with structural sharing.
//!
//! Zero third-party dependencies (only `std`). Replaces the subset of `im-rc`
//! the Niao runtime relies on:
//!
//! * [`Vector`] — a persistent, bit-partitioned vector trie (branching factor
//!   32). `push_back` and `update` return a logically-new collection while
//!   sharing untouched sub-trees with the original, so prior handles remain
//!   valid snapshots.
//! * [`HashMap`] — a persistent Hash Array Mapped Trie (HAMT), same sharing
//!   semantics.
//!
//! Structural sharing is achieved with `Rc::make_mut`: nodes reachable from more
//! than one snapshot are cloned on write, unique nodes are mutated in place.

mod map;
mod vector;

pub use map::HashMap;
pub use vector::Vector;

use std::hash::{Hash, Hasher};

/// 64-bit FNV-1a hasher — small, deterministic, and dependency-free.
pub struct FnvHasher(u64);

impl Default for FnvHasher {
    fn default() -> Self {
        FnvHasher(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for FnvHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut h = self.0;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        self.0 = h;
    }
}

/// Hash a key to a 64-bit value using the crate's FNV-1a hasher.
pub(crate) fn hash_key<K: Hash + ?Sized>(key: &K) -> u64 {
    let mut h = FnvHasher::default();
    key.hash(&mut h);
    h.finish()
}
