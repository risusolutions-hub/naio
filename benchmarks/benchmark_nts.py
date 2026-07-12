#!/usr/bin/env python3
"""Benchmark nts vs statsmodels. Run from repo root."""

import subprocess
import sys
import time

try:
    import numpy as np
    import statsmodels.api as sm
    from statsmodels.tsa.stattools import acf as sm_acf
except ImportError:
    print("numpy and statsmodels required: pip install numpy statsmodels")
    sys.exit(1)

N = 2000
rng = np.random.default_rng(42)
y = np.zeros(N)
y[:2] = [1.0, 0.5]
for t in range(2, N):
    y[t] = 0.6 * y[t - 1] - 0.3 * y[t - 2] + rng.normal(0, 0.1)

# statsmodels baseline: ACF nlags=40
t0 = time.perf_counter()
for _ in range(10):
    _ = sm_acf(y, nlags=40, fft=True)
sm_ms = (time.perf_counter() - t0) / 10 * 1000

# Rust micro-bench
rust_bench = r"""
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
"""

bench_path = "crates/niao_ts/examples/bench_acf.rs"
with open(bench_path, "w") as f:
    f.write(rust_bench)

result = subprocess.run(
    [
        "cargo", "run", "--manifest-path", "crates/niao_ts/Cargo.toml",
        "--release", "--example", "bench_acf",
    ],
    capture_output=True,
    text=True,
)

niao_ms = None
if result.returncode == 0:
    niao_ms = float(result.stdout.strip())
else:
    print("Rust bench failed:", result.stderr[:500])

print(f"=== benchmark_nts.py (N={N}, ACF nlags=40, 10 iter) ===")
print(f"statsmodels acf: {sm_ms:.2f} ms/iter")
if niao_ms is not None:
    ratio = niao_ms / sm_ms if sm_ms > 0 else float("inf")
    print(f"niao_ts acf:     {niao_ms:.2f} ms/iter  ({ratio:.2f}x vs statsmodels)")
else:
    print("niao_ts acf:     (run after workspace wiring)")

# ARIMA fit comparison
air = sm.datasets.get_rdataset("AirPassengers").data["Passengers"].values.astype(float)[:48]
t0 = time.perf_counter()
for _ in range(3):
    m = sm.tsa.ARIMA(air, order=(1, 1, 1)).fit()
sm_arima_ms = (time.perf_counter() - t0) / 3 * 1000
print(f"statsmodels ARIMA(1,1,1) fit (n=48): {sm_arima_ms:.1f} ms/iter")
