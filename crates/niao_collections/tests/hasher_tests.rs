//! Distribution / avalanche / seed-randomness tests for the ahash replacement.

use core::hash::{BuildHasher, Hasher};
use niao_collections::{HashMap, HashMapExt, HashSet, HashSetExt, RandomState};

fn hash(state: &RandomState, bytes: &[u8]) -> u64 {
    let mut h = state.build_hasher();
    h.write(bytes);
    h.finish()
}

/// Flipping any single input bit should change roughly half of the output bits
/// (strict avalanche criterion). We assert the mean over all input-bit flips
/// lands in a generous band around 32/64.
#[test]
fn avalanche_single_bit_flips() {
    let state = RandomState::with_seeds(0xDEAD_BEEF, 0x1234_5678, 0xCAFE_F00D, 0x0BAD_F00D);
    let base: Vec<u8> = (0..32u8).collect();
    let base_hash = hash(&state, &base);

    let mut total_changed = 0u64;
    let mut flips = 0u64;
    let mut min_changed = 64u32;
    let mut max_changed = 0u32;

    for byte in 0..base.len() {
        for bit in 0..8 {
            let mut m = base.clone();
            m[byte] ^= 1 << bit;
            let diff = (hash(&state, &m) ^ base_hash).count_ones();
            total_changed += u64::from(diff);
            min_changed = min_changed.min(diff);
            max_changed = max_changed.max(diff);
            flips += 1;
        }
    }

    let mean = total_changed as f64 / flips as f64;
    eprintln!("avalanche: mean={mean:.2} min={min_changed} max={max_changed} flips={flips}");
    // Mean must be close to 32 (half the bits).
    assert!(
        (28.0..=36.0).contains(&mean),
        "avalanche mean {mean} out of band"
    );
    // No flip may leave the hash nearly unchanged or nearly fully inverted.
    assert!(min_changed >= 12, "some flip barely changed output: {min_changed}");
    assert!(max_changed <= 52, "some flip nearly inverted output: {max_changed}");
}

/// Across many inputs, every output bit should be set roughly half the time
/// (no stuck bits).
#[test]
fn no_stuck_output_bits() {
    let state = RandomState::with_seeds(11, 22, 33, 44);
    let n = 20_000u64;
    let mut ones = [0u64; 64];
    for i in 0..n {
        let h = hash(&state, &i.to_le_bytes());
        for (b, slot) in ones.iter_mut().enumerate() {
            *slot += (h >> b) & 1;
        }
    }
    for (b, &c) in ones.iter().enumerate() {
        let frac = c as f64 / n as f64;
        assert!(
            (0.45..=0.55).contains(&frac),
            "bit {b} set {frac:.3} of the time (stuck/biased)"
        );
    }
}

/// Sequential integer keys must spread across buckets (low collision).
#[test]
fn sequential_keys_low_collision() {
    let state = RandomState::with_seeds(7, 7, 7, 7);
    const N: u64 = 50_000;
    const BUCKETS: usize = 1024;
    let mut counts = vec![0u32; BUCKETS];
    for i in 0..N {
        let h = hash(&state, &i.to_le_bytes());
        counts[(h as usize) % BUCKETS] += 1;
    }
    let expected = N as f64 / BUCKETS as f64;
    let max = *counts.iter().max().unwrap() as f64;
    // No bucket should be wildly overloaded (uniform-ish).
    assert!(max < 3.0 * expected, "max bucket {max} vs expected {expected}");
}

/// Two independently constructed `RandomState`s should hash the same key to
/// different values (per-process random seed), demonstrating HashDoS resistance.
#[test]
fn seed_randomness_between_states() {
    let key = b"the quick brown fox jumps over the lazy dog";
    let mut distinct = HashSet::<u64>::new();
    for _ in 0..16 {
        let s = RandomState::new();
        distinct.insert(hash(&s, key));
    }
    // Overwhelmingly likely all 16 differ; require at least most to differ.
    assert!(
        distinct.len() >= 15,
        "RandomState::new() not randomizing seeds: {} distinct",
        distinct.len()
    );
}

/// A single `RandomState` must be stable within its lifetime.
#[test]
fn same_state_is_stable() {
    let s = RandomState::new();
    let key = b"stable?";
    assert_eq!(hash(&s, key), hash(&s, key));
}

#[test]
fn map_and_set_roundtrip() {
    let mut m: HashMap<String, u32> = HashMapExt::with_capacity(8);
    for i in 0..1000u32 {
        m.insert(format!("k{i}"), i);
    }
    assert_eq!(m.len(), 1000);
    for i in 0..1000u32 {
        assert_eq!(m.get(&format!("k{i}")), Some(&i));
    }

    let mut s: HashSet<u64> = HashSetExt::new();
    for i in 0..1000u64 {
        s.insert(i % 500);
    }
    assert_eq!(s.len(), 500);
}
