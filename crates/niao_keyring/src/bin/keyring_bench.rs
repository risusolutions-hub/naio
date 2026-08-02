//! Micro-benchmarks for `niao_keyring` hot paths.
//! Run: cargo run -p niao_keyring --bin keyring_bench --release

use niao_keyring::{
    clear_memory, delete_password, get_password, set_password, use_memory, BackendMode,
};
use std::time::Instant;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        f();
    }
    clear_memory();
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
    use_memory();
    clear_memory();

    bench(
        "memory set_password x10k",
        || {
            for i in 0..10_000 {
                let svc = format!("bench-svc-{i}");
                set_password(&svc, "user", "secret-value").unwrap();
            }
            10_000
        },
        10_000,
    );

    clear_memory();
    for i in 0..10_000 {
        set_password(&format!("get-{i}"), "user", "v").unwrap();
    }
    bench(
        "memory get_password x10k",
        || {
            for i in 0..10_000 {
                let _ = get_password(&format!("get-{i}"), "user").unwrap();
            }
            10_000
        },
        10_000,
    );

    clear_memory();
    for i in 0..5_000 {
        set_password(&format!("del-{i}"), "user", "v").unwrap();
    }
    bench(
        "memory delete_password x5k",
        || {
            for i in 0..5_000 {
                let _ = delete_password(&format!("del-{i}"), "user");
            }
            5_000
        },
        5_000,
    );

    // One-shot system roundtrip (when OS store is available).
    niao_keyring::use_system();
    let svc = "niao-keyring-bench";
    let user = "bench-user";
    let _ = delete_password(svc, user);
    let start = Instant::now();
    if set_password(svc, user, "bench-secret").is_ok() {
        let _ = get_password(svc, user);
        let _ = delete_password(svc, user);
        let ns = start.elapsed().as_nanos();
        println!("system set/get/delete roundtrip: {ns} ns");
    } else {
        println!("system roundtrip: skipped (OS store unavailable in this environment)");
    }

    let _mode: BackendMode = niao_keyring::backend_mode();
}
