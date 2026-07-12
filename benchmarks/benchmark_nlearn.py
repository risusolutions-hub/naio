#!/usr/bin/env python3
"""Benchmark nlearn (via cargo example) vs scikit-learn on Iris + synthetic.

Usage:
  python benchmarks/benchmark_nlearn.py
"""
from __future__ import annotations

import subprocess
import time
from pathlib import Path

import numpy as np
from sklearn.datasets import load_iris, make_classification
from sklearn.ensemble import RandomForestClassifier
from sklearn.linear_model import LogisticRegression
from sklearn.neighbors import KNeighborsClassifier
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler
from sklearn.tree import DecisionTreeClassifier

ROOT = Path(__file__).resolve().parents[1]


def bench(name: str, fn, repeats: int = 5) -> float:
    times = []
    for _ in range(repeats):
        t0 = time.perf_counter()
        fn()
        times.append(time.perf_counter() - t0)
    ms = 1000.0 * float(np.median(times))
    print(f"{name:40s} {ms:8.3f} ms")
    return ms


def main() -> None:
    iris = load_iris()
    X, y = iris.data, iris.target
    Xb, yb = X, (y == 0).astype(float)

    print("=== sklearn (reference) ===")
    t_log = bench(
        "LogisticRegression fit+score Iris",
        lambda: LogisticRegression(C=1e12, solver="lbfgs", max_iter=1000)
        .fit(Xb, yb)
        .score(Xb, yb),
    )
    t_pipe = bench(
        "Pipeline(Scaler,LogReg) Iris",
        lambda: Pipeline(
            [
                ("sc", StandardScaler()),
                ("lr", LogisticRegression(C=1e12, solver="lbfgs", max_iter=1000)),
            ]
        )
        .fit(Xb, yb)
        .score(Xb, yb),
    )
    t_dt = bench(
        "DecisionTree depth=3 Iris",
        lambda: DecisionTreeClassifier(max_depth=3).fit(X, y).score(X, y),
    )
    t_rf = bench(
        "RandomForest 10 trees Iris",
        lambda: RandomForestClassifier(n_estimators=10, max_depth=3, random_state=42)
        .fit(X, y)
        .score(X, y),
    )
    t_knn = bench(
        "kNN k=5 Iris",
        lambda: KNeighborsClassifier(n_neighbors=5).fit(X, y).score(X, y),
    )

    Xs, ys = make_classification(
        n_samples=5000, n_features=20, n_informative=10, random_state=0
    )
    t_syn = bench(
        "LogReg synthetic 5k×20",
        lambda: LogisticRegression(C=1e12, solver="lbfgs", max_iter=200)
        .fit(Xs, ys)
        .score(Xs, ys),
        repeats=3,
    )

    print("\n=== niao_learn (cargo test / release bench binary) ===")
    # Time a release test run of parity suite as a coarse gate
    t0 = time.perf_counter()
    r = subprocess.run(
        ["cargo", "test", "-p", "niao_learn", "--lib", "--release", "--", "--test-threads=1"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    t_cargo = 1000.0 * (time.perf_counter() - t0)
    ok = r.returncode == 0
    print(f"{'cargo test -p niao_learn --release':40s} {t_cargo:8.1f} ms  ok={ok}")
    if not ok:
        print(r.stderr[-2000:])

    print("\nNotes:")
    print(f"  sklearn LogReg Iris ~ {t_log:.3f} ms; target nlearn within 3–5× after runtime bench binary.")
    print(f"  sklearn Pipe/DT/RF/kNN/syn: {t_pipe:.3f}/{t_dt:.3f}/{t_rf:.3f}/{t_knn:.3f}/{t_syn:.3f} ms")


if __name__ == "__main__":
    main()
