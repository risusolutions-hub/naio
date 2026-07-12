use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u64 = 100_000_000;

fn main() {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        black_box(tracing::enabled!(tracing::Level::TRACE));
    }
    let ns_per_call = start.elapsed().as_secs_f64() * 1_000_000_000.0 / ITERATIONS as f64;
    println!("tracing disabled level: {ns_per_call:.3} ns/call ({ITERATIONS} iterations)");
}
