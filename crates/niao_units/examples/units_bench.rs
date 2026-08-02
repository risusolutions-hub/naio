//! Release-mode micro-benchmark for quantity parse + convert hot path.

use niao_units::{parse_quantity, Registry};
use std::time::Instant;

fn main() {
    let reg = Registry::default();
    let iterations = 500_000usize;

    // Baseline: parse magnitude only (no unit) as f64 parse
    let t0 = Instant::now();
    let mut acc = 0.0f64;
    for i in 0..iterations {
        let s = format!("{}.5", i % 1000);
        acc += s.parse::<f64>().unwrap();
    }
    let baseline_ns = t0.elapsed().as_nanos() as f64 / iterations as f64;

    // nunits: parse "100 km", convert to m, add
    let t1 = Instant::now();
    let mut sum = 0.0f64;
    for _ in 0..iterations {
        let (mag, unit) = parse_quantity("100 km", &reg).unwrap();
        let q = niao_units::Quantity::new(mag, unit);
        let m = reg.lookup("m").unwrap();
        let converted = q.to_unit(&m).unwrap();
        sum += converted.magnitude();
    }
    let nunits_ns = t1.elapsed().as_nanos() as f64 / iterations as f64;

    println!("iterations: {iterations}");
    println!("baseline (f64 parse only): {baseline_ns:.1} ns/op");
    println!("nunits parse+convert:       {nunits_ns:.1} ns/op");
    println!("ratio: {:.2}x", nunits_ns / baseline_ns);
    println!("checksum: {sum}");
}
