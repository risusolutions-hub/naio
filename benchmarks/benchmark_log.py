#!/usr/bin/env python3
"""Compare disabled-level checks in tracing and niao_log."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BENCH_TARGET = ROOT / "target" / "log-bench-target"


def run(command: list[str]) -> int:
    print("Running:", " ".join(command), flush=True)
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(BENCH_TARGET)
    return subprocess.run(command, cwd=ROOT, env=env, check=False).returncode


def main() -> int:
    # This standalone fixture intentionally retains tracing until the
    # orchestrator's serialized dependency-removal pass.
    code = run(
        [
            "cargo",
            "run",
            "--release",
            "--manifest-path",
            str(ROOT / "benchmarks" / "tracing-baseline" / "Cargo.toml"),
        ]
    )
    if code:
        return code
    return run(["cargo", "run", "--release", "-p", "niao_log", "--bin", "log_bench"])


if __name__ == "__main__":
    sys.exit(main())
