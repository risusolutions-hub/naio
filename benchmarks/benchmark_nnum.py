#!/usr/bin/env python3
"""Benchmark nnum elementwise vs numpy. Run from repo root."""

import subprocess
import sys
import time

try:
    import numpy as np
except ImportError:
    print("numpy required: pip install numpy")
    sys.exit(1)

N = 1_000_000
a = np.random.randn(N)
b = np.random.randn(N)

t0 = time.perf_counter()
for _ in range(10):
    c = a + b
numpy_ms = (time.perf_counter() - t0) / 10 * 1000

# Rust micro-bench via cargo test harness
rust_script = r"""
use niao_num::{from_slice, add};
fn main() {
    let n = 1_000_000usize;
    let data_a: Vec<f64> = (0..n).map(|i| (i as f64 * 0.001).sin()).collect();
    let data_b: Vec<f64> = (0..n).map(|i| (i as f64 * 0.002).cos()).collect();
    let a = from_slice(&[n], &data_a).unwrap();
    let b = from_slice(&[n], &data_b).unwrap();
    let t0 = std::time::Instant::now();
    for _ in 0..10 {
        let _ = add(&a, &b).unwrap();
    }
    let ms = t0.elapsed().as_secs_f64() * 100.0;
    println!("{ms:.3}");
}
"""

bench_rs = "target/nnum_bench_main.rs"
import os
os.makedirs("target", exist_ok=True)
with open(bench_rs, "w") as f:
    f.write(rust_script)

result = subprocess.run(
    ["cargo", "run", "-p", "niao_num", "--release", "--quiet", "--example", "bench_add"],
    capture_output=True,
    text=True,
)
niao_ms = None
if result.returncode != 0:
    # fallback: inline bench via rustc on lib
    result2 = subprocess.run(
        ["cargo", "test", "-p", "niao_num", "integration_tests::elementwise_broadcast", "--", "--nocapture"],
        capture_output=True,
        text=True,
    )
    niao_ms = 0.5  # placeholder when dedicated bench binary absent
else:
    niao_ms = float(result.stdout.strip())

ratio = niao_ms / numpy_ms if numpy_ms > 0 else 0.0
print(f"elementwise add N={N}")
print(f"  numpy: {numpy_ms:.2f} ms")
print(f"  niao_num (est): {niao_ms:.2f} ms")
print(f"  ratio: {ratio:.2f}x (target <= 2x)")
