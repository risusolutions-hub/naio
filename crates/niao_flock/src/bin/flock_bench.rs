//! Micro-benchmarks for `niao_flock` hot paths.
//! Run: cargo run -p niao_flock --bin flock_bench --release

use niao_flock::{lock, LockHandle, LockMode, LockOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn temp_lock(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("niao_flock_bench");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(name)
}

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    println!(
        "{name}: n={n} mean={mean_ns:.0} ns total={} ms",
        elapsed.as_millis()
    );
}

fn main() {
    let path = temp_lock("bench.lock");
    let _ = std::fs::remove_file(&path);

    bench(
        "try_acquire/release x10k",
        || {
            let opts = LockOptions::default();
            for _ in 0..10_000 {
                let mut h = LockHandle::open(&path, &opts).unwrap();
                let _ = h.try_acquire(LockMode::Exclusive).unwrap();
                let _ = h.release();
            }
            10_000
        },
        10_000,
    );

    bench(
        "lock convenience x5k",
        || {
            let opts = LockOptions {
                timeout: Some(Duration::from_millis(0)),
                ..LockOptions::default()
            };
            for i in 0..5_000 {
                let p = temp_lock(&format!("conv_{i}.lock"));
                let mut h = lock(&p, &opts).unwrap();
                let _ = h.release();
                let _ = std::fs::remove_file(p);
            }
            5_000
        },
        5_000,
    );

    bench(
        "pid_alive x100k",
        || {
            let pid = std::process::id();
            for _ in 0..100_000 {
                let _ = niao_flock::pid_alive(pid);
            }
            100_000
        },
        100_000,
    );

    bench(
        "read_pid x10k",
        || {
            let p = temp_lock("pid_read.lock");
            niao_flock::write_pid(&p, None).unwrap();
            for _ in 0..10_000 {
                let _ = niao_flock::read_pid(&p).unwrap();
            }
            let _ = std::fs::remove_file(p);
            10_000
        },
        10_000,
    );

    let _ = std::fs::remove_file(path);
}
