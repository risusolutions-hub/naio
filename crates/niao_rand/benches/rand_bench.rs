//! Throughput benchmark: niao_rand vs `rand` 0.8 (dev-dependency).

use niao_rand::{Rng, SeedableRng, StdRng};
use std::time::Instant;

const ITERS: u64 = 5_000_000;

fn bench(name: &str, iters: u64, f: impl FnMut()) -> f64 {
    let mut f = f;
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let secs = start.elapsed().as_secs_f64();
    let ops_per_sec = iters as f64 / secs;
    println!("{name}: {ops_per_sec:.2e} ops/s ({iters} iters in {secs:.3}s)");
    ops_per_sec
}

fn main() {
    println!("=== niao_rand bench (release recommended) ===");

    let niao_u64 = {
        let mut rng = StdRng::seed_from_u64(42);
        bench("niao_next_u64", ITERS, || {
            std::hint::black_box(rng.next_u64());
        })
    };

    let niao_range = {
        let mut rng = StdRng::seed_from_u64(42);
        bench("niao_gen_range", ITERS, || {
            std::hint::black_box(rng.gen_range(0..1_000_000));
        })
    };

    let rand_u64 = {
        use rand::{RngCore, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        bench("rand_next_u64", ITERS, || {
            std::hint::black_box(rng.next_u64());
        })
    };

    let rand_range = {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        bench("rand_gen_range", ITERS, || {
            std::hint::black_box(rng.gen_range(0..1_000_000));
        })
    };

    let ratio_u64 = niao_u64 / rand_u64;
    let ratio_range = niao_range / rand_range;
    println!(
        "summary: niao/rand next_u64={ratio_u64:.2}x gen_range={ratio_range:.2}x (target >= 1.0x)"
    );
    if ratio_u64 < 1.0 || ratio_range < 1.0 {
        eprintln!("warning: below target on one or more ops");
    }
}
