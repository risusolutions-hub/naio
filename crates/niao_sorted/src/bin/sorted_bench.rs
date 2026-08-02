//! Native micro-benchmark for niao_sorted hot paths.

use niao_sorted::{SortValue, SortedList};
use std::time::Instant;

fn bench_inserts(n: usize) -> f64 {
    let mut list = SortedList::new();
    let t0 = Instant::now();
    for i in 0..n as i64 {
        list.add(SortValue::Int(i % (n as i64 / 10 + 1))).unwrap();
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_bisect(n: usize, queries: usize) -> f64 {
    let data: Vec<i64> = (0..n as i64).collect();
    let list = SortedList::from_ints(&data);
    let t0 = Instant::now();
    for q in 0..queries {
        let v = SortValue::Int((q % n) as i64);
        let _ = list.bisect_left(&v).unwrap();
        let _ = list.bisect_right(&v).unwrap();
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench_irange(n: usize, ranges: usize) -> f64 {
    let data: Vec<i64> = (0..n as i64).collect();
    let list = SortedList::from_ints(&data);
    let t0 = Instant::now();
    for r in 0..ranges {
        let lo = SortValue::Int((r % (n / 2)) as i64);
        let hi = SortValue::Int((r % (n / 2) + n / 4) as i64);
        let _ = list.irange(&lo, &hi, true, true).unwrap();
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let n = 100_000;
    println!("niao_sorted bench (n={n})");
    println!("  insert {} items: {:.2} ms", n, bench_inserts(n));
    println!(
        "  bisect {} queries: {:.2} ms",
        n * 2,
        bench_bisect(n, n * 2)
    );
    println!(
        "  irange {} ranges: {:.2} ms",
        n / 10,
        bench_irange(n, n / 10)
    );
}
