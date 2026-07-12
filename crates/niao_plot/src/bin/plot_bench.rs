//! 10k-point SVG render benchmark for benchmark_nplot.py

use niao_plot::{Figure, PlotResult};
use std::time::Instant;

fn main() -> PlotResult<()> {
    let n = 10_000usize;
    let x: Vec<f64> = (0..n).map(|i| i as f64 * 0.01).collect();
    let y: Vec<f64> = x.iter().map(|v| v.sin() * 10.0).collect();
    let mut fig = Figure::new(800.0, 600.0);
    fig.axes(0)?.line(&x, &y, Some("10k line"))?;
    let t0 = Instant::now();
    for _ in 0..5 {
        let _ = fig.to_svg_string();
    }
    let ms = t0.elapsed().as_secs_f64() / 5.0 * 1000.0;
    println!("{ms:.3}");
    Ok(())
}
