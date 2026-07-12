#!/usr/bin/env python3
"""Run the niao_collections hash bench vs the `ahash` crate (8/64/1k byte keys).

Requires the `ahash` crate (still present until the orchestrator removes it).
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    cmd = [
        "cargo",
        "run",
        "--release",
        "-p",
        "niao_collections",
        "--features",
        "compare-ahash",
        "--bin",
        "hash_bench",
    ]
    print("Running:", " ".join(cmd))
    proc = subprocess.run(cmd, cwd=ROOT, check=False)
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
