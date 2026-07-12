#!/usr/bin/env python3
"""Run niao_http HTTP types benchmark (release build)."""

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
        "niao_http",
        "--bin",
        "http_types_bench",
    ]
    print("Running:", " ".join(cmd))
    proc = subprocess.run(cmd, cwd=ROOT, check=False)
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
