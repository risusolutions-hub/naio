//! Elementwise add micro-benchmark for benchmark_nnum.py

use niao_num::{add, from_slice};
use std::time::Instant;

fn main() {
    let n = 1_000_000usize;
    let data_a: Vec<f64> = (0..n).map(|i| (i as f64 * 0.001).sin()).collect();
    let data_b: Vec<f64> = (0..n).map(|i| (i as f64 * 0.002).cos()).collect();
    let a = from_slice(&[n], &data_a).unwrap();
    let b = from_slice(&[n], &data_b).unwrap();
    let t0 = Instant::now();
    for _ in 0..10 {
        let _ = add(&a, &b).unwrap();
    }
    let ms = t0.elapsed().as_secs_f64() * 100.0;
    println!("{ms:.3}");
}
