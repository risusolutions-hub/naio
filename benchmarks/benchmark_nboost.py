#!/usr/bin/env python3
"""Benchmark nboost vs sklearn/LightGBM. Run from repo root."""

import subprocess
import sys
import time

try:
    import numpy as np
    from sklearn.ensemble import GradientBoostingRegressor
except ImportError:
    print("numpy and sklearn required: pip install numpy scikit-learn")
    sys.exit(1)

try:
    import lightgbm as lgb

    HAS_LGB = True
except ImportError:
    HAS_LGB = False

N_ROWS = 10_000
N_FEATURES = 50
N_ROUNDS = 100
SEED = 42

np.random.seed(SEED)
X = np.random.randn(N_ROWS, N_FEATURES)
y = X[:, 0] + 2 * X[:, 1] + 0.1 * np.random.randn(N_ROWS)

# sklearn baseline
t0 = time.perf_counter()
sk = GradientBoostingRegressor(
    n_estimators=N_ROUNDS,
    max_depth=6,
    learning_rate=0.1,
    min_samples_leaf=20,
    random_state=SEED,
)
sk.fit(X, y)
sklearn_ms = (time.perf_counter() - t0) * 1000
sk_pred = sk.predict(X)
sk_rmse = float(np.sqrt(np.mean((sk_pred - y) ** 2)))

# LightGBM baseline (optional)
lgb_ms = None
lgb_rmse = None
if HAS_LGB:
    t0 = time.perf_counter()
    ds = lgb.Dataset(X, label=y)
    params = {
        "objective": "regression",
        "learning_rate": 0.1,
        "max_depth": 6,
        "num_leaves": 31,
        "min_data_in_leaf": 20,
        "verbose": -1,
        "seed": SEED,
    }
    m = lgb.train(params, ds, num_boost_round=N_ROUNDS)
    lgb_ms = (time.perf_counter() - t0) * 1000
    lgb_pred = m.predict(X)
    lgb_rmse = float(np.sqrt(np.mean((lgb_pred - y) ** 2)))

# niao_boost via Rust example
result = subprocess.run(
    [
        "cargo",
        "run",
        "--manifest-path",
        "crates/niao_boost/Cargo.toml",
        "--release",
        "--example",
        "bench_boost",
    ],
    capture_output=True,
    text=True,
)
niao_ms = float(result.stdout.strip()) if result.returncode == 0 else None

print(f"GBDT train {N_ROWS}x{N_FEATURES}, {N_ROUNDS} rounds, seed={SEED}")
print(f"  sklearn:     {sklearn_ms:.1f} ms  train RMSE={sk_rmse:.6f}")
if lgb_ms is not None:
    print(f"  lightgbm:    {lgb_ms:.1f} ms  train RMSE={lgb_rmse:.6f}")
if niao_ms is not None:
    ratio = niao_ms / sklearn_ms if sklearn_ms > 0 else 0.0
    print(f"  niao_boost:  {niao_ms:.1f} ms  ratio vs sklearn={ratio:.2f}x")
else:
    print("  niao_boost:  (run failed)", result.stderr[:200])
