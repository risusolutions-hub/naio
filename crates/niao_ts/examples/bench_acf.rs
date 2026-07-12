//! ACF micro-benchmark (overwritten by benchmarks/benchmark_nts.py).

use niao_ts::acf;

fn main() {
    let n = 2000usize;
    let mut y = vec![0.0; n];
    y[0] = 1.0;
    y[1] = 0.5;
    for t in 2..n {
        y[t] = 0.6 * y[t - 1] - 0.3 * y[t - 2];
    }
    let t0 = std::time::Instant::now();
    for _ in 0..10 {
        let v = acf(&y, Some(40)).unwrap();
        std::hint::black_box(v);
    }
    println!("{:.3}", t0.elapsed().as_secs_f64() * 100.0);
}
