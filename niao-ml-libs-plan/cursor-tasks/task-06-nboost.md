# Task 06 — nboost: XGBoost / LightGBM (crate `niao_boost`)
Wave 2 (needs nnum, nframe). Read `../MASTER_PLAN.md` + `../specs/niao_boost__nboost.md`. Error block **4060–4069**.
Depends on: `nnum`, `nframe`, `neval`. **Correctness before speed.**

## Build (`crates/niao_boost`, zero new deps)
- **Pre-bin once:** quantile-bin each feature into ≤256 bins → u8/u16 codes, column-major. Split search reads bins, not floats.
- **Histogram tree learner:** per-node accumulate per-bin (Σg, Σh, count) in one pass; best split by cumulative scan;
  gain = ½[Gₗ²/(Hₗ+λ)+Gᵣ²/(Hᵣ+λ)−G²/(H+λ)]−γ. **Histogram subtraction** (child=parent−sibling, build smaller child).
  **Leaf-wise** frontier (priority queue by gain, cap max_leaves/max_depth) + depth-wise mode.
- Objectives: squared error, logistic, softmax (+ custom grad/hess hook). Grad/hess once per round.
- Reg: learning_rate, lambda(L2), alpha(L1), gamma, min_child_weight, subsample(row), colsample(feature), min_data_in_leaf.
- n_estimators + early stopping on val metric (neval) + best-iteration. Missing values: learned default direction.
- feature_importance(gain/split/cover), predict/predict_proba, save_model/load_model(njson). Same fit/predict shape as nlearn.
- Parallelize histogram build across features/threads (bounded pool); SIMD grad/predict passes with fallback.

## Wire up
- `niao_libs/nboost/` wrapper + builtins; `docs/NBOOST.md`; `examples/nboost_demo.niao` (train + predict on a fixture).

## Acceptance
- Histogram split gains == brute-force exact-split reference on tiny data (1e-9) BEFORE optimizing. Then:
  regression RMSE vs LightGBM within 2%; binary AUC/logloss within 2%; multiclass Iris/digits within 2%; early stop at
  expected iter; missing-value routing fixture; save→load→predict identical.
- predict-before-fit→4063, bad params→4064, X/y mismatch→4065.
- `benchmarks/benchmark_nboost.py` vs LightGBM/XGBoost; training within 3–5x on 100k×50, 100 rounds. `cargo test -p niao_boost` green.

See `../cursor-rules.md`.
