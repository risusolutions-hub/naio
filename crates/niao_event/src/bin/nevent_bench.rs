//! Micro-benchmarks for `niao_event` hot paths.
//! Run: cargo run -p niao_event --bin nevent_bench --release

use niao_event::{Emitter, EmitterOptions, TopicPattern};
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
    bench(
        "topic parse x100k",
        || {
            for i in 0..100_000 {
                let _ = TopicPattern::parse(if i % 2 == 0 {
                    "user.created"
                } else {
                    "order.*.paid"
                });
            }
            100_000
        },
        100_000,
    );

    bench(
        "exact match x100k",
        || {
            let p = TopicPattern::parse("user.created").unwrap();
            for _ in 0..100_000 {
                let _ = p.matches("user.created");
            }
            100_000
        },
        100_000,
    );

    bench(
        "wildcard match x100k",
        || {
            let p = TopicPattern::parse("user.**").unwrap();
            for _ in 0..100_000 {
                let _ = p.matches("user.admin.login");
            }
            100_000
        },
        100_000,
    );

    let mut em = Emitter::default();
    for _ in 0..50 {
        let _ = em.subscribe("exact.topic", false);
    }
    for i in 0..50 {
        let _ = em.subscribe(&format!("wild.{i}.*"), false);
    }

    bench(
        "matching_ids exact x50k",
        || {
            for _ in 0..50_000 {
                let _ = em.matching_ids("exact.topic");
            }
            50_000
        },
        50_000,
    );

    bench(
        "matching_ids wildcard x50k",
        || {
            for _ in 0..50_000 {
                let _ = em.matching_ids("wild.25.event");
            }
            50_000
        },
        50_000,
    );

    bench(
        "subscribe x10k",
        || {
            let mut e = Emitter::new(EmitterOptions {
                max_listeners_per_pattern: 0,
            });
            for i in 0..10_000 {
                let _ = e.subscribe(&format!("t.{i}"), false).unwrap();
            }
            10_000
        },
        10_000,
    );
}
