#!/usr/bin/env python3
"""Benchmark nvision 1000-image load→resize→normalize vs torchvision (optional).

Run from repo root:
  python benchmarks/benchmark_nvision.py
"""

from __future__ import annotations

import subprocess
import sys
import time

N = 1000
TARGET_RATIO = 2.0  # within 2× of torchvision


def run_niao() -> float:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--release",
            "-p",
            "niao_vision",
            "--bin",
            "vision_bench",
            "--quiet",
        ],
        capture_output=True,
        text=True,
        cwd=".",
    )
    if result.returncode != 0:
        print("niao_vision bench failed (workspace member may need wiring):")
        print(result.stderr)
        sys.exit(1)
    return float(result.stdout.strip().splitlines()[-1])


def run_torchvision() -> float | None:
    try:
        import torch
        from torchvision import transforms
        from PIL import Image
        import numpy as np
    except Exception:
        return None

    imgs = []
    for i in range(N):
        arr = np.zeros((64, 64, 3), dtype=np.uint8)
        arr[:] = ((i * 13) % 256)
        imgs.append(Image.fromarray(arr, mode="RGB"))

    tfm = transforms.Compose(
        [
            transforms.Resize((32, 32)),
            transforms.ToTensor(),
            transforms.Normalize([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]),
        ]
    )
    t0 = time.perf_counter()
    for im in imgs:
        _ = tfm(im)
    return (time.perf_counter() - t0) * 1000.0


def main() -> None:
    niao_ms = run_niao()
    print(f"1000-image resize(32)+normalize (synthetic 64×64 RGB)")
    print(f"  niao_vision: {niao_ms:.2f} ms")
    tv = run_torchvision()
    if tv is None:
        print("  torchvision: (not installed — skip ratio)")
        print("  PASS (niao bench ran)")
        return
    print(f"  torchvision: {tv:.2f} ms")
    ratio = niao_ms / max(tv, 1e-9)
    print(f"  ratio: {ratio:.2f}x (target <= {TARGET_RATIO:.0f}x)")
    if ratio > TARGET_RATIO:
        print("  FAIL: exceeds 2× torchvision")
        sys.exit(1)
    print("  PASS")


if __name__ == "__main__":
    main()
