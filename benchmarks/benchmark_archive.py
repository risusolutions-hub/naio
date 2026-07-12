#!/usr/bin/env python3
"""Run niao_archive throughput benchmark (1 MiB payload)."""

from __future__ import annotations

import subprocess
import sys
import time
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SIZE = 1024 * 1024
ITERS = 32


def payload() -> bytes:
    return bytes((i * 17 + i // 256) % 251 for i in range(SIZE))


def bench_zlib(data: bytes, gz: bytes) -> float:
    start = time.perf_counter()
    for _ in range(ITERS):
        zlib.decompress(gz)
    secs = time.perf_counter() - start
    mb = (SIZE * ITERS) / (1024 * 1024)
    return mb / secs


def main() -> int:
    data = payload()
    gz = zlib.compress(data, level=6)
    py_mib_s = bench_zlib(data, gz)
    print(f"python_zlib_inflate: {py_mib_s:.1} MiB/s (reference)")

    cmd = [
        "cargo",
        "run",
        "--release",
        "-p",
        "niao_archive",
        "--bin",
        "archive_bench",
    ]
    print("Running:", " ".join(cmd))
    proc = subprocess.run(cmd, cwd=ROOT, check=False)
    if proc.returncode != 0:
        return proc.returncode
    ratio = py_mib_s  # archive_bench prints its own numbers; gate is manual read
    print(f"reference zlib inflate: {ratio:.1} MiB/s (target niao >= 60% on same HW)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
