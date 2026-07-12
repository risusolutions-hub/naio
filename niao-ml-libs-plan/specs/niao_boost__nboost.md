# Library spec: `nboost`  →  crate `niao_boost`

| | |
|---|---|
| Category | Gradient boosting |
| Replaces (Python) | `XGBoost` / `LightGBM` |
| Rust reference | `gbdt-rs` |
| Target Niao crate | `crates/niao_boost` |
| Niao import name | `nboost` |
| Difficulty | 4/5 — Very Hard |
| Wave | 2 (needs nnum, nframe) |
| Depends on Niao libs | `nnum`, `nframe`, `neval` |
| Error block | 4060–4069 |

## Goal
A fast **histogram-based gradient-boosted decision tree** library — the workhorse of tabular ML/Kaggle —
matching a practical XGBoost/LightGBM subset. **Zero external deps.** Consumes `nframe`/`nnum` matrices;
metrics via `neval`.

## Scope (v1)
- **Tasks:** regression (L2, L1/pseudo-Huber), binary classification (logloss), multiclass (softmax,
  one-vs-all boosters), ranking (pairwise/LambdaMART — v2 if time-boxed).
- **Objectives / losses:** squared error, logistic, softmax; custom objective hook (grad + hess callback).
- **Tree learner:** **histogram binning** (feature values → fixed bins, e.g. 256) + **leaf-wise growth**
  (LightGBM style) with `max_leaves`, plus depth-wise mode; second-order split gain
  `gain = ½[ Gₗ²/(Hₗ+λ) + Gᵣ²/(Hᵣ+λ) − G²/(H+λ) ] − γ`.
- **Regularization:** `learning_rate` (eta), `lambda` (L2), `alpha` (L1), `gamma` (min split gain),
  `min_child_weight`, `max_depth`, `max_leaves`, `subsample` (row), `colsample` (feature), `min_data_in_leaf`.
- **Training:** `n_estimators`, **early stopping** on a validation metric, best-iteration tracking.
- **Missing values:** default-direction learning per split (XGBoost sparsity-aware).
- **Categorical features:** native handling via ordered/one-hot binning (basic; document limits).
- **Inspection:** `feature_importance` (gain/split/cover), `predict` / `predict_proba`, per-iteration eval log,
  `save_model`/`load_model` (JSON via njson).

## Implementation blueprint (make it FAST + LIGHT — this is where speed comes from)
- **Pre-bin once.** Quantile-bin each feature into ≤256 bins → store as `u8`/`u16` codes in a column-major matrix.
  All split search then reads bins, not floats — this is the LightGBM/XGBoost speed trick.
- **Histogram accumulation.** For a node, accumulate per-bin `(Σg, Σh, count)` in one pass over its rows;
  find the best split by scanning cumulative sums per feature. Reuse histogram buffers across nodes.
- **Histogram subtraction.** Child histogram = parent − sibling → build the smaller child, subtract for the larger.
- **Leaf-wise frontier.** Priority queue of (node, best_gain); expand max-gain leaf until `max_leaves`/`max_depth`.
- Gradients/Hessians computed once per boosting round from current predictions; row/feature subsampling via `nrand`.
- Parallelize histogram building across features/threads (std threads, bounded pool). Predictions vectorized over rows.

### Performance rules
- Bins are `u8`/`u16`, not `f64`. No float compares in the split loop. Reuse `(g,h)` and histogram buffers — no
  per-node allocation. `#[inline]` the histogram inner loop; SIMD the gradient/prediction passes with scalar fallback.

## Public API surface
`GBRegressor` / `GBClassifier` with `fit(x, y, eval_set?, early_stopping_rounds?)`, `predict`, `predict_proba`,
`feature_importance`, `save_model/load_model`, and a low-level `Booster` + `Dataset`. Same estimator shape as
`nlearn` (fit/predict/score). Expose to Niao via `niao_libs/nboost/` + builtins.

## Performance target
- Predictions match a LightGBM/XGBoost baseline within tolerance (same hyperparams, same seed): RMSE/AUC within
  1–2% on the benchmark datasets.
- Training wall-clock within **3–5×** of LightGBM on a 100k×50 dataset, 100 rounds (histogram path must be used).

## Tests required
- Regression on a synthetic function + a UCI fixture: RMSE vs LightGBM baseline within 2%, `n_estimators` fixed.
- Binary classification: AUC/logloss vs baseline within 2%; `predict_proba` in [0,1] and calibrated-ish.
- Multiclass on Iris/digits: accuracy within 2% of LightGBM.
- Early stopping halts at the expected best iteration on a seeded val split.
- Histogram correctness: split gains vs a brute-force exact-split reference on a tiny dataset, `rtol=1e-9`.
- Missing-value default direction: a fixture with NaNs routes as expected.
- `save_model`→`load_model`→`predict` reproduces predictions exactly.
- Degenerate: predict before fit → 4063; bad params (bins<2, eta≤0) → 4064; X/y mismatch → 4065.
- Plus: in-crate unit tests, `examples/nboost_demo.niao`, `benchmarks/benchmark_nboost.py` vs LightGBM/XGBoost.

## Risk / notes
- **Correctness before speed.** Get exact-split gains right on tiny data first, then switch to histograms and prove
  they agree, then optimize. Histogram subtraction bugs are subtle — test child = parent − sibling directly.
- Ranking/LambdaMART and GPU are v2. DART/goss optional v2.
- Numerical: clamp Hessians away from 0; use `min_child_weight`/`lambda` to keep leaf values finite.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_boost` green; parity vs LightGBM/XGBoost within tolerance.
- Histogram path is the default and demonstrably faster than the exact-split fallback.
- `niao_libs/nboost/` wrapper + `examples/nboost_demo.niao` trains + predicts on a fixture.
- Benchmark + notes in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
