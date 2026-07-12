#!/usr/bin/env python3
"""Run niao_net_clients FTP throughput benchmark vs suppaftp."""

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
        "niao_net_clients",
        "--test",
        "ftp_bench",
        "--",
        "--nocapture",
    ]
    print("Running:", " ".join(cmd))
    proc = subprocess.run(cmd, cwd=ROOT, check=False)
    return proc.returncode


if __name__ == "__main__":
    sys.exit(main())
