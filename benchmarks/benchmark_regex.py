#!/usr/bin/env python3
"""Compare niao_regex vs optional regex crate on email-like scan."""

import subprocess
import sys
import time

HAY = ("user@example.com and " * 200_000).encode()
PATTERN = br"[\w.+-]+@[\w.-]+\.\w+"

def run_niao():
    start = time.perf_counter()
    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--release", "-p", "niao_regex", "--bin", "regex_bench"],
        cwd=".",
        capture_output=True,
        text=True,
    )
    secs = time.perf_counter() - start
    if proc.returncode != 0:
        print(proc.stderr, file=sys.stderr)
        sys.exit(1)
    print(proc.stdout.strip())
    print(f"wall_time: {secs:.2f}s")


if __name__ == "__main__":
    run_niao()
