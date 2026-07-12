//! Fast, non-cryptographic hashing — a zero-dependency replacement for the
//! `ahash` crate.
//!
//! * On `x86_64` **compiled with AES-NI** (`-C target-feature=+aes`, or
//!   `-C target-cpu=native`), the hot loop uses `aesenc` intrinsics with two
//!   parallel lanes to hide instruction latency. This matches how `ahash`
//!   itself selects AES (compile-time `cfg`), so the intrinsics fully inline.
//! * Everywhere else a wyhash-style multiply fallback is used — fully inlined,
//!   with dedicated integer fast-paths. Both paths share the same public API.
//! * [`RandomState`] pulls a per-process random seed from the OS RNG (via
//!   `std`'s SipHash keys) so hash outputs are unpredictable to an attacker
//!   (HashDoS resistance), while remaining stable within a single process.
//!
//! API parity with `ahash`: [`RandomState`], [`AHasher`], the [`HashMap`] /
//! [`HashSet`] type aliases, and the [`HashMapExt`] / [`HashSetExt`] extension
//! traits (`::new()` / `::with_capacity()` sugar).

use core::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// Digits of pi — arbitrary, well-mixed constants used to key the hashers.
const PI: [u64; 4] = [
    0x243f_6a88_85a3_08d3,
    0x1319_8a2e_0370_7344,
    0xa409_3822_299f_31d0,
    0x082e_fa98_ec4e_6c89,
];

/// True when the AES-NI hash path is compiled in.
pub const USES_AES: bool = cfg!(all(target_arch = "x86_64", target_feature = "aes"));

/// wyhash mixing primitive: 64x64 -> 128 multiply folded to 64 bits.
#[inline(always)]
fn wymix(a: u64, b: u64) -> u64 {
    let r = (a as u128).wrapping_mul(b as u128);
    (r as u64) ^ ((r >> 64) as u64)
}

#[inline(always)]
fn read_u64(data: &[u8]) -> u64 {
    // Caller guarantees `data.len() >= 8`.
    u64::from_le_bytes(data[..8].try_into().unwrap())
}

/// Read 1..=8 bytes into a single `u64` without reading out of bounds.
#[inline(always)]
fn read_small(data: &[u8]) -> u64 {
    let len = data.len();
    if len >= 4 {
        let a = u32::from_le_bytes(data[..4].try_into().unwrap()) as u64;
        let b = u32::from_le_bytes(data[len - 4..].try_into().unwrap()) as u64;
        (a << 32) | b
    } else if len > 0 {
        // 1..=3 bytes: first, middle, last.
        ((data[0] as u64) << 16) | ((data[len >> 1] as u64) << 8) | (data[len - 1] as u64)
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Per-process seed
// ---------------------------------------------------------------------------

static GLOBAL_SEED: OnceLock<[u64; 4]> = OnceLock::new();
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Extract a random `u64` from the OS entropy pool. `std`'s `RandomState`
/// seeds its SipHash keys from the OS RNG, so hashing a fixed value with a
/// freshly built hasher yields fresh randomness on each call.
fn os_random_u64() -> u64 {
    use std::hash::BuildHasher as _;
    let rs = std::collections::hash_map::RandomState::new();
    let mut h = rs.build_hasher();
    h.write_u64(0x9e37_79b9_7f4a_7c15);
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.finish()
}

#[inline]
fn global_seed() -> [u64; 4] {
    *GLOBAL_SEED.get_or_init(|| {
        [
            os_random_u64(),
            os_random_u64(),
            os_random_u64(),
            os_random_u64(),
        ]
    })
}

// ---------------------------------------------------------------------------
// Scalar fallback hash state (wyhash-style, fully inlined)
// ---------------------------------------------------------------------------

#[cfg(not(all(target_arch = "x86_64", target_feature = "aes")))]
mod state_impl {
    use super::{read_small, read_u64, wymix, PI};

    #[derive(Clone)]
    pub struct HashState {
        hash: u64,
        hash2: u64,
        key1: u64,
        key2: u64,
        len: u64,
    }

    impl HashState {
        #[inline(always)]
        pub fn new(keys: [u64; 4]) -> Self {
            HashState {
                hash: keys[0] ^ PI[0],
                hash2: keys[1] ^ PI[1],
                key1: keys[2] ^ PI[2],
                key2: keys[3] ^ PI[3],
                len: 0,
            }
        }

        #[inline(always)]
        pub fn write(&mut self, mut d: &[u8]) {
            self.len = self.len.wrapping_add(d.len() as u64);
            // Fast path for short keys (≤16 bytes): at most two reads, one mix,
            // no two-lane bookkeeping.
            if d.len() <= 16 {
                if d.len() >= 8 {
                    let a = read_u64(&d[..8]);
                    let b = if d.len() > 8 { read_small(&d[8..]) } else { 0 };
                    self.hash = wymix(self.hash ^ a, self.key1 ^ b);
                } else if !d.is_empty() {
                    let a = read_small(d);
                    self.hash = wymix(self.hash ^ a, self.key1);
                }
                return;
            }
            // Two independent lanes so the 64x64 multiplies pipeline (ILP).
            let mut h0 = self.hash;
            let mut h1 = self.hash2;
            while d.len() >= 32 {
                let a = read_u64(&d[..8]);
                let b = read_u64(&d[8..16]);
                let c = read_u64(&d[16..24]);
                let e = read_u64(&d[24..32]);
                h0 = wymix(h0 ^ a, self.key1 ^ b);
                h1 = wymix(h1 ^ c, self.key2 ^ e);
                d = &d[32..];
            }
            if d.len() >= 16 {
                let a = read_u64(&d[..8]);
                let b = read_u64(&d[8..16]);
                h0 = wymix(h0 ^ a, self.key1 ^ b);
                d = &d[16..];
            }
            if d.len() >= 8 {
                let a = read_u64(&d[..8]);
                h1 = wymix(h1 ^ a, self.key2);
                d = &d[8..];
            }
            if !d.is_empty() {
                let a = read_small(d);
                h0 = wymix(h0 ^ a, self.key1);
            }
            self.hash = h0;
            self.hash2 = h1;
        }

        #[inline(always)]
        pub fn write_u64(&mut self, i: u64) {
            self.len = self.len.wrapping_add(8);
            self.hash = wymix(self.hash ^ i, self.key1);
        }

        #[inline(always)]
        pub fn finish(&self) -> u64 {
            wymix(
                self.hash ^ self.len,
                self.hash2 ^ self.key1 ^ self.key2 ^ PI[0],
            )
        }
    }
}

// ---------------------------------------------------------------------------
// AES-NI hash state (x86_64 compiled with +aes). Two parallel lanes.
// ---------------------------------------------------------------------------

#[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
mod state_impl {
    use super::PI;
    use core::arch::x86_64::*;

    #[derive(Clone, Copy)]
    pub struct HashState {
        a: __m128i,
        b: __m128i,
        key: __m128i,
        len: u64,
    }

    impl HashState {
        #[inline(always)]
        pub fn new(keys: [u64; 4]) -> Self {
            // SAFETY: crate compiled with +aes ⇒ SSE2 is available.
            unsafe {
                HashState {
                    a: _mm_set_epi64x((keys[1] ^ PI[1]) as i64, (keys[0] ^ PI[0]) as i64),
                    b: _mm_set_epi64x((keys[3] ^ PI[3]) as i64, (keys[2] ^ PI[2]) as i64),
                    key: _mm_set_epi64x((keys[0] ^ PI[2]) as i64, (keys[3] ^ PI[1]) as i64),
                    len: 0,
                }
            }
        }

        #[inline(always)]
        pub fn write(&mut self, mut d: &[u8]) {
            self.len = self.len.wrapping_add(d.len() as u64);
            // SAFETY: AES-NI + SSE2 guaranteed by the +aes build.
            unsafe {
                // Fast path for short keys: build the block from two scalar
                // reads (no zeroed temp buffer, no branchy tail).
                if d.len() <= 16 {
                    if !d.is_empty() {
                        let (lo, hi) = if d.len() >= 8 {
                            let lo = super::read_u64(&d[..8]);
                            let hi = if d.len() > 8 {
                                super::read_small(&d[8..])
                            } else {
                                0
                            };
                            (lo, hi)
                        } else {
                            (super::read_small(d), 0)
                        };
                        let blk = _mm_set_epi64x(hi as i64, lo as i64);
                        self.a = _mm_aesenc_si128(self.a, blk);
                    }
                    return;
                }
                while d.len() >= 32 {
                    let b0 = _mm_loadu_si128(d.as_ptr() as *const __m128i);
                    let b1 = _mm_loadu_si128(d[16..].as_ptr() as *const __m128i);
                    self.a = _mm_aesenc_si128(self.a, b0);
                    self.b = _mm_aesenc_si128(self.b, b1);
                    d = &d[32..];
                }
                if d.len() >= 16 {
                    let b0 = _mm_loadu_si128(d.as_ptr() as *const __m128i);
                    self.a = _mm_aesenc_si128(self.a, b0);
                    d = &d[16..];
                }
                if !d.is_empty() {
                    let mut tmp = [0u8; 16];
                    tmp[..d.len()].copy_from_slice(d);
                    let b0 = _mm_loadu_si128(tmp.as_ptr() as *const __m128i);
                    self.b = _mm_aesenc_si128(self.b, b0);
                }
            }
        }

        #[inline(always)]
        pub fn write_u64(&mut self, i: u64) {
            self.len = self.len.wrapping_add(8);
            // SAFETY: see `write`.
            unsafe {
                self.a = _mm_aesenc_si128(self.a, _mm_set_epi64x(0, i as i64));
            }
        }

        #[inline(always)]
        pub fn finish(&self) -> u64 {
            // SAFETY: see `write`.
            unsafe {
                let lenv = _mm_set_epi64x(self.len as i64, self.len.rotate_left(32) as i64);
                // Diffuse each lane independently, then combine and mix once more.
                // Every input bit therefore passes through >= 2 AES rounds
                // (full 128-bit diffusion) counting the per-block round in `write`.
                let a2 = _mm_aesenc_si128(self.a, lenv);
                let combined = _mm_aesenc_si128(self.b, a2);
                let h = _mm_aesenc_si128(combined, self.key);
                let mut out = [0u64; 2];
                _mm_storeu_si128(out.as_mut_ptr() as *mut __m128i, h);
                out[0] ^ out[1].rotate_left(23)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public hasher
// ---------------------------------------------------------------------------

/// A fast, non-cryptographic hasher. Constructed via [`RandomState`].
#[derive(Clone)]
pub struct AHasher {
    state: state_impl::HashState,
}

impl AHasher {
    #[inline(always)]
    fn from_keys(keys: [u64; 4]) -> Self {
        AHasher {
            state: state_impl::HashState::new(keys),
        }
    }
}

impl Default for AHasher {
    /// Deterministic (fixed-key) hasher. Use [`RandomState`] for HashDoS
    /// resistance in maps.
    #[inline]
    fn default() -> Self {
        AHasher::from_keys(PI)
    }
}

impl Hasher for AHasher {
    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        self.state.write(bytes);
    }

    #[inline(always)]
    fn write_u8(&mut self, i: u8) {
        self.state.write_u64(i as u64);
    }

    #[inline(always)]
    fn write_u16(&mut self, i: u16) {
        self.state.write_u64(i as u64);
    }

    #[inline(always)]
    fn write_u32(&mut self, i: u32) {
        self.state.write_u64(i as u64);
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        self.state.write_u64(i);
    }

    #[inline(always)]
    fn write_usize(&mut self, i: usize) {
        self.state.write_u64(i as u64);
    }

    #[inline(always)]
    fn finish(&self) -> u64 {
        self.state.finish()
    }
}

/// A [`BuildHasher`] that seeds each [`AHasher`] from a per-process random key,
/// mirroring `ahash::RandomState`.
#[derive(Clone)]
pub struct RandomState {
    keys: [u64; 4],
}

impl RandomState {
    /// Random, per-process seed (HashDoS resistant). Distinct instances get
    /// distinct keys so parallel maps don't share a hash pattern.
    #[inline]
    pub fn new() -> Self {
        let g = global_seed();
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mix = c.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(1);
        RandomState {
            keys: [
                g[0] ^ mix,
                g[1].wrapping_add(mix).rotate_left(17),
                g[2] ^ mix.rotate_left(31),
                g[3].wrapping_add(mix.rotate_left(43)),
            ],
        }
    }

    /// Deterministic state derived from a single seed (reproducible; useful for
    /// tests). Not HashDoS resistant.
    #[inline]
    pub fn with_seed(seed: usize) -> Self {
        let s = seed as u64;
        RandomState {
            keys: [
                wymix(s ^ PI[0], PI[1]),
                wymix(s ^ PI[1], PI[2]),
                wymix(s ^ PI[2], PI[3]),
                wymix(s ^ PI[3], PI[0]),
            ],
        }
    }

    /// Fully explicit, deterministic keys.
    #[inline]
    pub fn with_seeds(k0: u64, k1: u64, k2: u64, k3: u64) -> Self {
        RandomState {
            keys: [k0, k1, k2, k3],
        }
    }
}

impl Default for RandomState {
    #[inline]
    fn default() -> Self {
        RandomState::new()
    }
}

impl BuildHasher for RandomState {
    type Hasher = AHasher;

    #[inline(always)]
    fn build_hasher(&self) -> AHasher {
        AHasher::from_keys(self.keys)
    }
}

impl core::fmt::Debug for RandomState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RandomState { .. }")
    }
}

// ---------------------------------------------------------------------------
// Convenience: hash a byte slice
// ---------------------------------------------------------------------------

/// Hash a byte slice with a fixed (deterministic) key. For quick, stable
/// hashing where HashDoS resistance is not required.
#[inline]
pub fn hash_bytes(data: &[u8]) -> u64 {
    let mut h = AHasher::default();
    h.write(data);
    h.finish()
}

// ---------------------------------------------------------------------------
// HashMap / HashSet aliases + `ahash`-style extension traits
// ---------------------------------------------------------------------------

/// Drop-in replacement for `ahash::HashMap` (a `std` map keyed by [`RandomState`]).
pub type HashMap<K, V> = std::collections::HashMap<K, V, RandomState>;
/// Drop-in replacement for `ahash::HashSet`.
pub type HashSet<K> = std::collections::HashSet<K, RandomState>;

/// `new()` / `with_capacity()` sugar for [`HashMap`], mirroring `ahash::HashMapExt`.
pub trait HashMapExt {
    /// Empty map with a fresh random hasher.
    fn new() -> Self;
    /// Preallocated map with a fresh random hasher.
    fn with_capacity(capacity: usize) -> Self;
}

/// `new()` / `with_capacity()` sugar for [`HashSet`], mirroring `ahash::HashSetExt`.
pub trait HashSetExt {
    /// Empty set with a fresh random hasher.
    fn new() -> Self;
    /// Preallocated set with a fresh random hasher.
    fn with_capacity(capacity: usize) -> Self;
}

impl<K, V> HashMapExt for HashMap<K, V> {
    #[inline]
    fn new() -> Self {
        std::collections::HashMap::with_hasher(RandomState::new())
    }
    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        std::collections::HashMap::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

impl<K> HashSetExt for HashSet<K> {
    #[inline]
    fn new() -> Self {
        std::collections::HashSet::with_hasher(RandomState::new())
    }
    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        std::collections::HashSet::with_capacity_and_hasher(capacity, RandomState::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::hash::Hash;

    fn hash_with(state: &RandomState, bytes: &[u8]) -> u64 {
        let mut h = state.build_hasher();
        h.write(bytes);
        h.finish()
    }

    #[test]
    fn deterministic_for_same_seed() {
        let s = RandomState::with_seeds(1, 2, 3, 4);
        assert_eq!(hash_with(&s, b"hello world"), hash_with(&s, b"hello world"));
    }

    #[test]
    fn different_inputs_differ() {
        let s = RandomState::with_seeds(1, 2, 3, 4);
        assert_ne!(hash_with(&s, b"hello"), hash_with(&s, b"hellp"));
        assert_ne!(hash_with(&s, b""), hash_with(&s, b"\0"));
    }

    #[test]
    fn handles_all_small_lengths() {
        let s = RandomState::with_seeds(9, 8, 7, 6);
        let buf: Vec<u8> = (0..64u8).collect();
        let mut seen = std::collections::HashSet::new();
        for len in 0..=buf.len() {
            // Must not panic for any length and should be well-distributed.
            seen.insert(hash_with(&s, &buf[..len]));
        }
        // Almost all lengths should produce distinct hashes.
        assert!(seen.len() >= buf.len() - 1);
    }

    #[test]
    fn map_alias_and_ext_traits_work() {
        let mut m: HashMap<String, i32> = HashMapExt::new();
        m.insert("a".into(), 1);
        m.insert("b".into(), 2);
        assert_eq!(m.get("a"), Some(&1));
        assert_eq!(m.get("b"), Some(&2));

        let mut s: HashSet<i64> = HashSetExt::with_capacity(16);
        s.insert(10);
        s.insert(10);
        s.insert(20);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn hash_one_convenience() {
        let s = RandomState::with_seeds(42, 42, 42, 42);
        let a = s.hash_one(1234u64);
        let b = s.hash_one(1234u64);
        let c = s.hash_one(1235u64);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    fn full_hash<T: Hash>(state: &RandomState, v: T) -> u64 {
        state.hash_one(v)
    }

    #[test]
    fn integer_keys_distribute() {
        let s = RandomState::with_seeds(0x1234, 0x5678, 0x9abc, 0xdef0);
        const BUCKETS: usize = 256;
        const N: usize = 100_000;
        let mut counts = [0u32; BUCKETS];
        for i in 0..N as u64 {
            let h = full_hash(&s, i);
            counts[(h as usize) % BUCKETS] += 1;
        }
        let expected = (N / BUCKETS) as f64;
        // Chi-square goodness of fit; for 255 dof, 1.5x expected is very loose.
        let chi: f64 = counts
            .iter()
            .map(|&c| {
                let d = c as f64 - expected;
                d * d / expected
            })
            .sum();
        assert!(chi < 1.5 * BUCKETS as f64, "chi-square too high: {chi}");
    }
}
