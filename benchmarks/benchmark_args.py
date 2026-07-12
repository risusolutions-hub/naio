#!/usr/bin/env python3
"""Compare niao_args vs clap on repeated CLI parse (parse-once workload)."""

import subprocess
import sys
import time


def run_bench():
    start = time.perf_counter()
    proc = subprocess.run(
        [
            "cargo",
            "test",
            "--quiet",
            "--release",
            "-p",
            "niao_args",
            "bench_parse_throughput",
            "--",
            "--nocapture",
        ],
        cwd=".",
        capture_output=True,
        text=True,
    )
    wall = time.perf_counter() - start
    if proc.returncode != 0:
        print(proc.stderr, file=sys.stderr)
        sys.exit(1)
    print(proc.stdout.strip())
    print(f"wall_time: {wall:.2f}s")


if __name__ == "__main__":
    run_bench()
