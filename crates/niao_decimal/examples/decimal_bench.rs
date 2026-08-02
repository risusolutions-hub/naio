//! Micro-benchmark for niao_decimal hot paths.

use niao_decimal::{parse_decimal, Context, Decimal, RoundingMode};
use std::time::Instant;

fn main() {
    let ctx = Context::new(28, RoundingMode::HalfEven);
    let n = 200_000usize;

    let start = Instant::now();
    let mut acc = Decimal::zero();
    for _ in 0..n {
        let d = parse_decimal("19.995").unwrap();
        let tax = parse_decimal("0.0825").unwrap().mul(&d, &ctx).unwrap();
        let total = d.add(&tax, &ctx).unwrap().quantize(-2, &ctx).unwrap();
        acc = acc.add(&total, &ctx).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "decimal_ops: {} iters in {:.3} ms ({:.0} ops/s)",
        n,
        elapsed.as_secs_f64() * 1000.0,
        n as f64 / elapsed.as_secs_f64()
    );
    println!("checksum: {}", acc);
}
