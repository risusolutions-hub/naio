//! Micro-benchmarks for `niao_id` hot paths.
//! Run: cargo run -p niao_id --bin nid_bench --release

use niao_id::{
    hashids::Hashids, nanoid, nanoid_bulk, ulid::Ulid, uuid4, uuid6, uuid7, HashidsError,
    MonotonicUlid, SnowflakeGenerator,
};
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
    let hashids =
        Hashids::new("bench-salt", 0, niao_id::HASHIDS_DEFAULT_ALPHABET).expect("hashids");

    bench(
        "uuid4 x100k",
        || {
            for _ in 0..100_000 {
                let _ = uuid4().to_string();
            }
            100_000
        },
        100_000,
    );

    bench(
        "uuid7 x100k",
        || {
            for _ in 0..100_000 {
                let _ = uuid7().to_string();
            }
            100_000
        },
        100_000,
    );

    bench(
        "uuid6 x100k",
        || {
            for _ in 0..100_000 {
                let _ = uuid6().to_string();
            }
            100_000
        },
        100_000,
    );

    bench(
        "ulid x100k",
        || {
            for _ in 0..100_000 {
                let _ = Ulid::new().to_string();
            }
            100_000
        },
        100_000,
    );

    bench(
        "ulid monotonic x100k",
        || {
            let mut gen = MonotonicUlid::new();
            for _ in 0..100_000 {
                let _ = gen.next().to_string();
            }
            100_000
        },
        100_000,
    );

    bench(
        "nanoid x100k",
        || {
            for _ in 0..100_000 {
                let _ = nanoid();
            }
            100_000
        },
        100_000,
    );

    bench(
        "nanoid_bulk 10x10k",
        || {
            for _ in 0..10 {
                let _ = nanoid_bulk(10_000, 21, niao_id::NANOID_DEFAULT_ALPHABET).unwrap();
            }
            100_000
        },
        100_000,
    );

    bench(
        "snowflake x100k",
        || {
            let gen = SnowflakeGenerator::new(1, 1).unwrap();
            for _ in 0..100_000 {
                let _ = gen.next_id().unwrap();
            }
            100_000
        },
        100_000,
    );

    bench(
        "hashids encode x50k",
        || {
            for i in 0..50_000u64 {
                let _ = hashids.encode(&[i, i + 1]).unwrap();
            }
            50_000
        },
        50_000,
    );

    bench(
        "hashids decode x50k",
        || {
            let samples: Vec<String> = (0..1000u64)
                .map(|i| hashids.encode(&[i, i + 1]).unwrap())
                .collect();
            for _ in 0..50 {
                for s in &samples {
                    let _: Result<Vec<u64>, HashidsError> = hashids.decode(s);
                }
            }
            50_000
        },
        50_000,
    );
}
