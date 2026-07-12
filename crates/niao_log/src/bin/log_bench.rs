use niao_log::{enabled, Level};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u64 = 100_000_000;
const TARGET_NS_PER_CALL: f64 = 5.0;

fn main() {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(enabled(black_box(Level::Trace)));
    }
    let elapsed = start.elapsed();
    let ns_per_call = elapsed.as_secs_f64() * 1_000_000_000.0 / ITERATIONS as f64;
    println!("niao_log disabled level: {ns_per_call:.3} ns/call ({ITERATIONS} iterations)");
    if ns_per_call > TARGET_NS_PER_CALL {
        eprintln!(
            "disabled-level target missed: {ns_per_call:.3} ns/call > {TARGET_NS_PER_CALL:.1} ns/call"
        );
        std::process::exit(1);
    }
}
