//! Micro-benchmarks for `niao_tune` hot paths.
//! Run: cargo run -p niao_tune --bin ntune_bench --release

use niao_tune::{
    grid_cartesian, grid_size, run_grid, sample_random, train_test_split_indices, ParamValue,
    SearchOpts, SpaceDim,
};
use std::collections::BTreeMap;
use std::time::Instant;

fn bench<F: FnMut()>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops_per_sec = 1e9 / mean_ns;
    println!(
        "{name}: iters={iters} mean={mean_ns:.1} ns/op ({ops_per_sec:.0} ops/s) total={} ms",
        elapsed.as_millis()
    );
}

fn grid_space_3x3x3() -> BTreeMap<String, SpaceDim> {
    let mut s = BTreeMap::new();
    s.insert(
        "a".into(),
        SpaceDim::Grid((0..3).map(|n| ParamValue::Int(n)).collect()),
    );
    s.insert(
        "b".into(),
        SpaceDim::Grid((0..3).map(|n| ParamValue::Int(n)).collect()),
    );
    s.insert(
        "c".into(),
        SpaceDim::Grid((0..3).map(|n| ParamValue::Int(n)).collect()),
    );
    s
}

fn main() {
    let space = grid_space_3x3x3();

    bench(
        "grid_size 3^3",
        || {
            let _ = grid_size(&space).unwrap();
        },
        5_000_000,
    );

    bench(
        "grid_cartesian 3^3 (27 combos)",
        || {
            let _ = grid_cartesian(&space).unwrap();
        },
        200_000,
    );

    bench(
        "sample_random n=100 dim=3",
        || {
            let _ = sample_random(&space, 100, 42).unwrap();
        },
        50_000,
    );

    bench(
        "train_test_split n=10000",
        || {
            let _ = train_test_split_indices(10_000, 0.2, 7).unwrap();
        },
        100_000,
    );

    let opts = SearchOpts::default();
    bench(
        "run_grid 3^3 + objective",
        || {
            let _ = run_grid(
                &space,
                |params| {
                    let sum = params
                        .values()
                        .map(|v| match v {
                            ParamValue::Int(n) => *n as f64,
                            _ => 0.0,
                        })
                        .sum::<f64>();
                    Ok(sum)
                },
                &opts,
            )
            .unwrap();
        },
        50_000,
    );

    // Naive baseline: manual nested loops for 3^3 grid (no allocation reuse).
    bench(
        "naive triple loop 3^3 (baseline)",
        || {
            let mut sum = 0.0;
            for a in 0..3 {
                for b in 0..3 {
                    for c in 0..3 {
                        sum += (a + b + c) as f64;
                    }
                }
            }
            std::hint::black_box(sum);
        },
        5_000_000,
    );
}
