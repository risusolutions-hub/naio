#!/usr/bin/env python3
"""Compare niao_json_core vs serde_json on ~5 MiB mixed JSON (release builds)."""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(cmd: list[str]) -> str:
    out = subprocess.check_output(cmd, cwd=ROOT, text=True, stderr=subprocess.STDOUT)
    return out


def main() -> None:
    print("=== benchmark_json.py ===")
    print(run(["cargo", "run", "--release", "-p", "niao_json_core", "--bin", "json_bench"]))
    # Optional serde_json baseline when available in tree
    try:
        baseline = ROOT / "benchmarks" / "benchmark_json_serde.py"
        if baseline.exists():
            run([sys.executable, str(baseline)])
    except Exception as e:
        print(f"(serde baseline skipped: {e})")


if __name__ == "__main__":
    main()
