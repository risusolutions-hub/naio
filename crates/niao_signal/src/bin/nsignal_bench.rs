//! Micro-benchmarks for `niao_signal` hot paths.
//! Run: cargo run -p niao_signal --bin nsignal_bench --release

use niao_signal::{self, HandlerKind};
use std::time::Instant;

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
    let sigint = niao_signal::parse_signal_name("sigint").unwrap();
    let sigterm = niao_signal::parse_signal_name("sigterm").unwrap();
    let _ = niao_signal::set_handler_kind(sigint, HandlerKind::Watched);
    let _ = niao_signal::set_handler_kind(sigterm, HandlerKind::Watched);

    bench(
        "name lookup x100k",
        || {
            for _ in 0..100_000 {
                let _ = niao_signal::signal_name(sigint);
            }
            100_000
        },
        100_000,
    );

    bench(
        "number parse x100k",
        || {
            for _ in 0..100_000 {
                let _ = niao_signal::parse_signal_name("SIGTERM");
            }
            100_000
        },
        100_000,
    );

    bench(
        "valid_signals",
        || {
            for _ in 0..10_000 {
                let _ = niao_signal::valid_signals();
            }
            10_000
        },
        10_000,
    );

    bench(
        "info strsignal x50k",
        || {
            for _ in 0..50_000 {
                let _ = niao_signal::strsignal(sigint);
            }
            50_000
        },
        50_000,
    );

    bench(
        "peek_pending empty x100k",
        || {
            for _ in 0..100_000 {
                let _ = niao_signal::peek_pending();
            }
            100_000
        },
        100_000,
    );

    bench(
        "drain_pending empty x100k",
        || {
            for _ in 0..100_000 {
                let _ = niao_signal::drain_pending();
            }
            100_000
        },
        100_000,
    );

    let _ = niao_signal::reset_all();
}
