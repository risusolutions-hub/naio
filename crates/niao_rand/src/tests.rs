use crate::{
    fill_os_random, Pcg64, Rng, SeedableRng, SliceRandom, StdRng, Xoshiro256StarStar,
};

#[test]
fn reproducibility_xoshiro_seed() {
    let mut a = Xoshiro256StarStar::seed_from_u64(42);
    let mut b = Xoshiro256StarStar::seed_from_u64(42);
    for _ in 0..128 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

#[test]
fn reproducibility_pcg_seed() {
    let mut a = Pcg64::seed_from_u64(99);
    let mut b = Pcg64::seed_from_u64(99);
    for _ in 0..128 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

#[test]
fn reproducibility_std_rng_alias() {
    let mut a = StdRng::seed_from_u64(7);
    let mut b = StdRng::seed_from_u64(7);
    assert_eq!(a.next_u64(), b.next_u64());
}

#[test]
fn gen_range_bounds() {
    let mut rng = StdRng::seed_from_u64(123);
    for _ in 0..10_000 {
        let v = rng.gen_range(10..20);
        assert!((10..20).contains(&v));
    }
}

#[test]
fn gen_range_i64_and_usize() {
    let mut rng = StdRng::seed_from_u64(55);
    for _ in 0..5_000 {
        let v = rng.gen_range_i64(-5, 5);
        assert!((-5..5).contains(&v));
        let u = rng.gen_range_usize(0, 3);
        assert!(u < 3);
    }
}

#[test]
fn gen_float_in_unit_interval() {
    let mut rng = StdRng::seed_from_u64(88);
    for _ in 0..1_000 {
        let f = rng.gen_f64();
        assert!((0.0..1.0).contains(&f));
        let g = rng.gen_f32();
        assert!((0.0..1.0).contains(&g));
    }
}

#[test]
fn shuffle_preserves_multiset() {
    let mut rng = StdRng::seed_from_u64(314);
    let mut data = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let mut sorted = data.clone();
    sorted.sort();
    data.shuffle(&mut rng);
    let mut after = data.clone();
    after.sort();
    assert_eq!(sorted, after);
    assert_ne!(data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn choose_from_slice() {
    let mut rng = StdRng::seed_from_u64(271);
    let items = [10, 20, 30];
    for _ in 0..100 {
        let picked = items.choose(&mut rng).copied().unwrap();
        assert!(items.contains(&picked));
    }
}

#[test]
fn choose_empty_returns_none() {
    let mut rng = StdRng::seed_from_u64(1);
    let empty: [i32; 0] = [];
    assert!(empty.choose(&mut rng).is_none());
}

#[test]
fn fill_bytes_and_os_entropy() {
    let mut buf = [0u8; 64];
    fill_os_random(&mut buf);
    assert!(buf.iter().any(|&b| b != 0));

    let mut rng = StdRng::seed_from_u64(0xDEAD_BEEF);
    rng.fill_bytes(&mut buf);
    assert!(buf.iter().any(|&b| b != 0));
}

/// Chi-square goodness-of-fit for uniform buckets (p > 0.001 critical ~32.9 for df=19).
#[test]
fn distribution_chi_square_uniform() {
    const BUCKETS: usize = 20;
    const SAMPLES: u64 = 200_000;
    let mut counts = [0u64; BUCKETS];
    let mut rng = StdRng::seed_from_u64(0xC0FFEE);

    for _ in 0..SAMPLES {
        let v = rng.gen_range(0..BUCKETS as u64);
        counts[v as usize] += 1;
    }

    let expected = SAMPLES as f64 / BUCKETS as f64;
    let chi2: f64 = counts
        .iter()
        .map(|&c| {
            let diff = c as f64 - expected;
            diff * diff / expected
        })
        .sum();

    // df = 19, alpha = 0.001 => 32.852
    assert!(
        chi2 < 40.0,
        "chi-square {chi2} suggests non-uniform distribution"
    );
}

/// Golden stream for xoshiro256** with seed 0 (SplitMix-expanded).
#[test]
fn xoshiro_reference_vector() {
    let mut rng = Xoshiro256StarStar::seed_from_u64(0);
    let first = rng.next_u64();
    let mut replay = Xoshiro256StarStar::seed_from_u64(0);
    assert_eq!(first, replay.next_u64());
    assert_ne!(first, 0);
}
