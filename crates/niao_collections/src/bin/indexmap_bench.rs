//! 1M insert/get/remove microbench: niao IndexMap vs indexmap crate.

use niao_collections::indexmap::{FxBuildHasher, IndexMap as NiaoMap};
use std::time::Instant;

const N: usize = 1_000_000;
const ROUNDS: u32 = 3;

fn bench_niao() -> (f64, f64, f64) {
    let mut insert_ns = 0.0f64;
    let mut get_ns = 0.0f64;
    let mut remove_ns = 0.0f64;
    for _ in 0..ROUNDS {
        let mut m = NiaoMap::with_capacity_and_hasher(N, FxBuildHasher);
        let t0 = Instant::now();
        for i in 0..N {
            m.insert(i, i);
        }
        insert_ns += t0.elapsed().as_secs_f64();

        let t1 = Instant::now();
        let mut sum = 0usize;
        for i in 0..N {
            sum = sum.wrapping_add(*m.get(&i).unwrap());
        }
        std::hint::black_box(sum);
        get_ns += t1.elapsed().as_secs_f64();

        let t2 = Instant::now();
        for i in (0..N).step_by(2) {
            let _ = m.swap_remove(&i);
        }
        remove_ns += t2.elapsed().as_secs_f64();
    }
    (
        insert_ns / f64::from(ROUNDS),
        get_ns / f64::from(ROUNDS),
        remove_ns / f64::from(ROUNDS),
    )
}

fn bench_crate() -> (f64, f64, f64) {
    use indexmap::IndexMap as CrateMap;
    let mut insert_ns = 0.0f64;
    let mut get_ns = 0.0f64;
    let mut remove_ns = 0.0f64;
    for _ in 0..ROUNDS {
        let mut m = CrateMap::with_capacity(N);
        let t0 = Instant::now();
        for i in 0..N {
            m.insert(i, i);
        }
        insert_ns += t0.elapsed().as_secs_f64();

        let t1 = Instant::now();
        let mut sum = 0usize;
        for i in 0..N {
            sum = sum.wrapping_add(*m.get(&i).unwrap());
        }
        std::hint::black_box(sum);
        get_ns += t1.elapsed().as_secs_f64();

        let t2 = Instant::now();
        for i in (0..N).step_by(2) {
            let _ = m.swap_remove(&i);
        }
        remove_ns += t2.elapsed().as_secs_f64();
    }
    (
        insert_ns / f64::from(ROUNDS),
        get_ns / f64::from(ROUNDS),
        remove_ns / f64::from(ROUNDS),
    )
}

fn rate(secs: f64) -> f64 {
    (N as f64) / secs
}

fn main() {
    println!("=== indexmap bench: 1M keys, {ROUNDS} rounds (release recommended) ===");
    // Warmup
    {
        let mut m = NiaoMap::with_capacity_fx(10_000);
        for i in 0..10_000 {
            m.insert(i, i);
        }
    }

    let (ni, ng, nr) = bench_niao();
    let (ci, cg, cr) = bench_crate();

    println!(
        "niao_collections IndexMap: insert={:.1} ops/s ({:.3}s) get={:.1} ops/s ({:.3}s) swap_remove={:.1} ops/s ({:.3}s)",
        rate(ni),
        ni,
        rate(ng),
        ng,
        rate(nr),
        nr
    );
    println!(
        "indexmap crate:            insert={:.1} ops/s ({:.3}s) get={:.1} ops/s ({:.3}s) swap_remove={:.1} ops/s ({:.3}s)",
        rate(ci),
        ci,
        rate(cg),
        cg,
        rate(cr),
        cr
    );

    let insert_ratio = rate(ni) / rate(ci);
    let get_ratio = rate(ng) / rate(cg);
    let rem_ratio = rate(nr) / rate(cr);
    println!(
        "summary: niao/crate insert={insert_ratio:.2}x get={get_ratio:.2}x swap_remove={rem_ratio:.2}x"
    );
    if insert_ratio < 1.0 || get_ratio < 1.0 {
        eprintln!("WARN: niao IndexMap slower than indexmap on insert or get");
        std::process::exit(1);
    }
}
