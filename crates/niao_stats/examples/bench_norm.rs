use niao_stats::dist::Normal;
use niao_stats::special::{norm_cdf, norm_pdf};
fn main() {
    let n = 100_000usize;
    let xs: Vec<f64> = (0..n).map(|i| -3.0 + 6.0 * i as f64 / n as f64).collect();
    let norm = Normal::standard();
    let t0 = std::time::Instant::now();
    for _ in 0..5 {
        let mut s = 0.0;
        for &x in &xs {
            s += norm.pdf(x) + norm.cdf(x);
        }
        std::hint::black_box(s);
    }
    println!("{:.3}", t0.elapsed().as_secs_f64() * 200.0);
}
