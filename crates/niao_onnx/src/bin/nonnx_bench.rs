//! Release-mode micro-benchmarks for niao_onnx.
use niao_onnx::{inspect_path, load_path, matmul_naive, SessionOptions};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn bench(name: &str, warmup: u32, iters: u32, mut f: impl FnMut()) {
    for _ in 0..warmup {
        f();
    }
    let t0 = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = t0.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = if ns > 0.0 { 1_000_000_000.0 / ns } else { 0.0 };
    println!("{name}: {iters} runs, {ns:.0} ns/op ({ops:.1} ops/s)");
}

fn main() {
    let path = fixture("mobilenetv2-7.onnx");
    if !path.exists() {
        eprintln!("fixture missing: {}", path.display());
        std::process::exit(1);
    }

    bench("inspect_path mobilenet", 3, 50, || {
        let _ = inspect_path(&path).unwrap();
    });

    let session = load_path(&path, &SessionOptions::default()).unwrap();
    let input = session.inputs()[0].clone();
    let shape: Vec<usize> = input.shape.iter().map(|d| d.unwrap_or(1)).collect();
    let n: usize = shape.iter().product();
    let data = vec![0.0f32; n];
    let mut feed = HashMap::new();
    feed.insert(input.name.clone(), (shape.clone(), data.clone()));

    bench("run_f32 mobilenet (1x3x224x224 zeros)", 1, 10, || {
        let _ = session.run_f32(&feed).unwrap();
    });

    bench("load_path mobilenet compile", 0, 5, || {
        let _ = load_path(&path, &SessionOptions::default()).unwrap();
    });

    let m = 64usize;
    let k = 64usize;
    let n = 64usize;
    let a: Vec<f32> = (0..m * k).map(|i| (i as f32 * 0.001).sin()).collect();
    let b: Vec<f32> = (0..k * n).map(|i| (i as f32 * 0.002).cos()).collect();
    bench("matmul_naive 64x64x64 baseline", 3, 200, || {
        let _ = matmul_naive(&a, &b, m, k, n);
    });
}
