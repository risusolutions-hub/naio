#!/usr/bin/env python3
"""Run niao_codec throughput benchmark (1 MiB payload)."""

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
        "niao_codec",
        "--bin",
        "codec_bench",
    ]
    print("Running:", " ".join(cmd))
    proc = subprocess.run(cmd, cwd=ROOT, check=False)
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
