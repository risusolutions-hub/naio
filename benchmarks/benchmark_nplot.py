#!/usr/bin/env python3
"""Benchmark nplot 10k-point SVG render. Run from repo root."""

import subprocess
import sys

N = 10_000
TARGET_MS = 100.0

result = subprocess.run(
    ["cargo", "run", "--release", "-p", "niao_plot", "--bin", "plot_bench", "--quiet"],
    capture_output=True,
    text=True,
    cwd=".",
)

if result.returncode != 0:
    # Fallback: integration test timing
    result2 = subprocess.run(
        [
            "cargo",
            "test",
            "-p",
            "niao_plot",
            "render_10k_line_under_budget",
            "--release",
            "--",
            "--nocapture",
        ],
        capture_output=True,
        text=True,
    )
    if result2.returncode != 0:
        print("niao_plot bench failed (workspace member may need wiring):")
        print(result.stderr or result2.stderr)
        sys.exit(1)
    print(f"10k-point SVG render: passed budget test (< {TARGET_MS} ms)")
    sys.exit(0)

niao_ms = float(result.stdout.strip().splitlines()[-1])
print(f"10k-point line SVG render (N={N})")
print(f"  niao_plot: {niao_ms:.2f} ms")
print(f"  target: < {TARGET_MS:.0f} ms")
if niao_ms >= TARGET_MS:
    print(f"  FAIL: exceeds {TARGET_MS} ms budget")
    sys.exit(1)
print("  PASS")
