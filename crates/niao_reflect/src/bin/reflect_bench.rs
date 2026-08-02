//! Micro-benchmarks for `niao_reflect` hot paths.
//! Run: cargo run -p niao_reflect --bin reflect_bench --release

use niao_reflect::{doc_from_source, parse_module_info, scan_sources_parallel};
use std::time::Instant;

const SAMPLE: &str = r#"
// Computes n factorial recursively.
// >>> fact(5)
// => 120
fn fact(n: int) -> int {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}

// Adds two values.
fn add(a: int, b: int) -> int {
    return a + b
}

struct Point {
    x: float
    y: float
}

class Counter {
    fn inc(self) {
        return 1
    }
}
"#;

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
        "{name}: iters={iters} mean={mean_ns:.0} ns total={} ms",
        elapsed.as_millis()
    );
}

fn main() {
    bench(
        "doc_from_source x200k",
        || {
            let _ = doc_from_source(SAMPLE, "add");
        },
        200_000,
    );

    bench(
        "parse_module_info x20k",
        || {
            let _ = parse_module_info(SAMPLE);
        },
        20_000,
    );

    let sources: Vec<(String, String)> = (0..64)
        .map(|i| (format!("mod_{i}.niao"), SAMPLE.to_string()))
        .collect();

    bench(
        "scan_sources_parallel 64 modules x200",
        || {
            let _ = scan_sources_parallel(&sources);
        },
        200,
    );
}
