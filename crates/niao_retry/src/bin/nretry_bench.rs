//! Micro-benchmarks for `niao_retry` hot paths.
//! Run: cargo run -p niao_retry --bin nretry_bench --release

use niao_rand::thread_rng;
use niao_retry::{compute_wait_ms, exponential_raw, RetryPolicy};
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
    println!(
        "{name}: iters={iters} mean={mean_ns:.1} ns total={} ms",
        elapsed.as_millis()
    );
}

fn main() {
    let policy = RetryPolicy::default();
    let mut rng = thread_rng();

    bench(
        "exponential_raw",
        || {
            let _ = exponential_raw(5, &policy);
        },
        5_000_000,
    );

    bench(
        "compute_wait_ms exp+full jitter",
        || {
            let _ = compute_wait_ms(4, &policy, policy.min_wait_ms, &mut rng);
        },
        2_000_000,
    );

    let mut no_jitter = policy.clone();
    no_jitter.jitter = niao_retry::JitterKind::None;
    bench(
        "compute_wait_ms no jitter",
        || {
            let _ = compute_wait_ms(4, &no_jitter, no_jitter.min_wait_ms, &mut rng);
        },
        5_000_000,
    );

    let mut decor = policy.clone();
    decor.strategy = niao_retry::BackoffStrategy::Decorrelated;
    bench(
        "compute_wait_ms decorrelated",
        || {
            let _ = compute_wait_ms(6, &decor, decor.min_wait_ms, &mut rng);
        },
        2_000_000,
    );
}
