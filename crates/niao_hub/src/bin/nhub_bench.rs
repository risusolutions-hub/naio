//! Micro-benchmarks for nhub hot paths.
//! Run: cargo run -p niao_hub --bin nhub_bench --release

use niao_crypto::{hex, sha256};
use niao_glob::match_str;
use niao_hub::{hash_bytes, HashAlgo};
use std::time::Instant;

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        samples.push(t0.elapsed().as_nanos() as u64);
        let _ = n;
    }
    samples.sort_unstable();
    let mean: u64 = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    let ops_per_sec = if mean > 0 {
        1_000_000_000.0 / mean as f64
    } else {
        0.0
    };
    println!("{name}: mean={mean} ns/op ({ops_per_sec:.0} ops/s) p50={p50} ns (n={iters})");
}

fn bench_throughput<F: Fn() -> usize>(name: &str, bytes: usize, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let _ = f();
        samples.push(t0.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let mean_ns: u64 = samples.iter().sum::<u64>() / iters as u64;
    let mb_s = if mean_ns > 0 {
        (bytes as f64) / (mean_ns as f64 / 1e9) / (1024.0 * 1024.0)
    } else {
        0.0
    };
    println!("{name}: {mb_s:.1} MB/s mean={mean_ns} ns ({bytes} bytes/op, n={iters})");
}

fn main() {
    let warmup = 3u32;
    let iters = 50u32;

    bench(
        "glob match *.safetensors",
        || {
            match_str("models/7B/model.safetensors", "*.safetensors", false).unwrap_or(false)
                as usize
        },
        warmup,
        iters,
    );

    let payload = vec![0xABu8; 1 << 20]; // 1 MiB
    bench_throughput(
        "sha256 1 MiB (niao_crypto)",
        payload.len(),
        || hex::encode(&sha256(&payload)).len(),
        warmup,
        iters,
    );

    bench_throughput(
        "sha256 1 MiB (nhub hash_bytes)",
        payload.len(),
        || hash_bytes(&payload, HashAlgo::Sha256).len(),
        warmup,
        iters,
    );

    // Naive baseline: re-hash via one-shot without streaming API reuse
    bench_throughput(
        "sha256 1 MiB naive (fresh slice each iter)",
        payload.len(),
        || {
            let tmp = payload.clone();
            hash_bytes(&tmp, HashAlgo::Sha256).len()
        },
        warmup,
        iters,
    );
}
