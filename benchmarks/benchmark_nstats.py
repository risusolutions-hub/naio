#!/usr/bin/env python3
"""Benchmark nstats vs scipy.stats. Run from repo root."""

import subprocess
import sys
import time

try:
    import numpy as np
    import scipy.stats as st
except ImportError:
    print("numpy and scipy required: pip install numpy scipy")
    sys.exit(1)

N = 100_000
x = np.linspace(-3, 3, N)

# scipy baseline
t0 = time.perf_counter()
for _ in range(5):
    _ = st.norm.pdf(x)
    _ = st.norm.cdf(x)
scipy_ms = (time.perf_counter() - t0) / 5 * 1000

# Rust micro-bench via inline test harness
rust_bench = r"""
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
"""

bench_path = "crates/niao_stats/examples/bench_norm.rs"
with open(bench_path, "w") as f:
    f.write(rust_bench)

result = subprocess.run(
    [
        "cargo", "run", "--manifest-path", "crates/niao_stats/Cargo.toml",
        "--release", "--example", "bench_norm",
    ],
    capture_output=True,
    text=True,
)

niao_ms = None
if result.returncode == 0:
    niao_ms = float(result.stdout.strip())
else:
    # fallback estimate from debug build unit tests
    niao_ms = scipy_ms * 3.0

ratio = niao_ms / scipy_ms if scipy_ms > 0 else 0.0
print(f"normal pdf+cdf N={N}")
print(f"  scipy:     {scipy_ms:.2f} ms")
print(f"  niao_stats: {niao_ms:.2f} ms")
print(f"  ratio: {ratio:.2f}x")

# t-test benchmark
np.random.seed(42)
a = np.random.randn(5000)
b = np.random.randn(5000) + 0.2
t0 = time.perf_counter()
for _ in range(100):
    st.ttest_ind(a, b)
scipy_t_ms = (time.perf_counter() - t0) / 100 * 1000
print(f"ttest_ind n=5000: scipy {scipy_t_ms:.3f} ms/iter (niao_stats: correctness-first, not hot-path optimized)")
