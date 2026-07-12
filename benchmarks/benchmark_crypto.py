#!/usr/bin/env python3
"""Run niao_crypto SHA-256 100 MiB benchmark (release build)."""

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

if __name__ == "__main__":
    out = subprocess.check_output(
        ["cargo", "run", "--release", "-p", "niao_crypto", "--bin", "crypto_bench"],
        cwd=ROOT,
        text=True,
    )
    print(out)
