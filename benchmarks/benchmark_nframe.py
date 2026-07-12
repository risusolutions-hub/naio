#!/usr/bin/env python3
"""Benchmark nframe groupby+join vs pandas on 1M rows. Run from repo root."""

import os
import subprocess
import sys
import tempfile
import time

try:
    import pandas as pd
    import numpy as np
except ImportError:
    print("pandas+numpy required: pip install pandas numpy")
    sys.exit(1)

N = 1_000_000
rng = np.random.default_rng(42)
keys = rng.integers(0, 10_000, size=N)
vals = rng.standard_normal(N)
right_keys = rng.integers(0, 10_000, size=N // 10)
right_vals = rng.standard_normal(N // 10)

left = pd.DataFrame({"k": keys, "v": vals})
right = pd.DataFrame({"k": right_keys, "w": right_vals})

t0 = time.perf_counter()
g = left.groupby("k", sort=True)["v"].agg(["sum", "mean"])
pandas_gb_ms = (time.perf_counter() - t0) * 1000

t0 = time.perf_counter()
j = left.merge(right, on="k", how="inner")
pandas_join_ms = (time.perf_counter() - t0) * 1000

td = tempfile.mkdtemp(prefix="nframe_bench_")
left_csv = os.path.join(td, "left.csv")
right_csv = os.path.join(td, "right.csv")
left.to_csv(left_csv, index=False)
right.to_csv(right_csv, index=False)

env = os.environ.copy()
env["NFRAME_LEFT_CSV"] = left_csv
env["NFRAME_RIGHT_CSV"] = right_csv

result = subprocess.run(
    [
        "cargo",
        "run",
        "--manifest-path",
        "crates/niao_frame/Cargo.toml",
        "--release",
        "--quiet",
        "--example",
        "bench_groupby_join",
    ],
    capture_output=True,
    text=True,
    env=env,
)

if result.returncode != 0:
    print("Rust bench failed:")
    print(result.stderr[-1200:] if result.stderr else result.stdout[-1200:])
    sys.exit(1)

parts = result.stdout.strip().split()
niao_gb_ms = float(parts[0])
niao_join_ms = float(parts[1])

def ratio(a, b):
    return a / b if b > 0 else float("nan")

print(f"N={N}")
print(
    f"groupby sum+mean: pandas {pandas_gb_ms:.2f} ms | nframe {niao_gb_ms:.2f} ms | "
    f"ratio {ratio(niao_gb_ms, pandas_gb_ms):.2f}x (target <= 3x)"
)
print(
    f"inner join:       pandas {pandas_join_ms:.2f} ms | nframe {niao_join_ms:.2f} ms | "
    f"ratio {ratio(niao_join_ms, pandas_join_ms):.2f}x (target <= 3x)"
)
print(f"pandas join rows: {len(j)}  groupby groups: {len(g)}")
