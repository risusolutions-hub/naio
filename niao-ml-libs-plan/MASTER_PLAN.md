# Niao ML Libraries — MASTER PLAN

Goal: add **10 native machine-learning libraries** to Niao so it has a complete scientific-ML
stack around the existing `niao_tensor` + `niao_ml` (the PyTorch side) and the existing
`ntune` / `neval` / `ntok` training helpers. Every library is `n`-prefixed, **std-only,
zero new third-party crates**, lightweight, and fast. Detailed per-library specs live in
`specs/` (one MD each). This file is the map + execution order.

Total libraries: **10**  |  New Rust crates: **10**  |  Waves: **3**

## What already exists (do NOT rebuild)

The deep-learning side is already covered — build **beside** it, not on top:

- `crates/niao_tensor` — tensor engine (autograd, CPU SIMD, optional CUDA). ≈ `torch.Tensor` / candle-core.
- `crates/niao_ml` — layers, optimizers, trainer, loss, dataloader, GNN. ≈ `torch.nn`.
- `crates/niao_data` — columnar prep, normalize, split, tensorize. (nframe wraps/extends this — reuse it.)
- `crates/niao_rag`, `niao_llm`, `niao_ml_models` — RAG / llama / transformers.
- `ntune` (LR schedules, k-fold, hyperparameter search), `neval` (accuracy/precision/recall/F1, MAE/MSE/RMSE/R²),
  `ntok` (BPE tokenizer). **nlearn, nts, nnlp must call these instead of duplicating them.**

## The 10 new libraries

| # | Lib | Crate | Replaces (Python) | Difficulty | Wave | Depends on (Niao) |
|---|-----|-------|-------------------|:---:|:---:|---|
| 1 | `nnum`    | `niao_num`    | numpy, scipy.linalg, scipy.fft | 4/5 | 0 | niao_tensor (GEMM), nrand |
| 2 | `nframe`  | `niao_frame`  | pandas / polars | 4/5 | 1 | nnum, niao_data, ncsv, njson |
| 3 | `nstats`  | `niao_stats`  | scipy.stats, statsmodels | 3/5 | 1 | nnum, nrand |
| 4 | `noptim`  | `niao_optim`  | scipy.optimize | 3/5 | 1 | nnum |
| 5 | `nplot`   | `niao_plot`   | matplotlib / seaborn | 3/5 | 1 | nnum (+ nframe optional) |
| 6 | `nlearn`  | `niao_learn`  | scikit-learn | 5/5 | 2 | nnum, nframe, nstats, nrand, neval |
| 7 | `nboost`  | `niao_boost`  | XGBoost / LightGBM | 4/5 | 2 | nnum, nframe |
| 8 | `nts`     | `niao_ts`     | statsmodels.tsa / prophet | 4/5 | 2 | nnum, nstats, noptim, nframe |
| 9 | `nnlp`    | `niao_nlp`    | nltk / gensim | 4/5 | 2 | nnum, nframe, ntok, nlearn |
| 10| `nvision` | `niao_vision` | torchvision / OpenCV | 4/5 | 2 | nnum, niao_tensor, niao_ml, ncodec |

Difficulty legend: 3/5 hard · 4/5 very hard · 5/5 extreme.

## Reality tiers — read before you start

- **GREEN (rewrite freely, pure Rust is fine):** nstats, noptim, nplot, nframe, nnlp, nts. Classical
  algorithms with well-known reference implementations; correctness-first, perf-second.
- **AMBER (large but doable, do incrementally):** nnum (linalg decompositions — SVD/QR/eig are fiddly),
  nlearn (breadth is the cost — ship estimators one family at a time behind a stable `fit/predict` trait),
  nboost (histogram GBDT — get correctness vs a reference first, then optimize), nvision (many transforms).
- **RED (scope-trap — cap v1, do NOT gold-plate):**
  - `nnum` full LAPACK parity — a std-only SVD/eig that is *correct and stable* is the goal; matching
    OpenBLAS throughput is a person-year. Target nalgebra-level, not MKL-level. Gate advanced decomps behind v2.
  - `nvision` pretrained backbones — implement transforms + conv building blocks + dataset loaders; do
    **not** hand-port ResNet/ViT weights in v1 (that's `niao_ml_models` territory — call it).
  - `nnlp` neural embeddings — classical NLP (TF-IDF, n-grams, count-vectors, word2vec-CBOW/skip-gram) only;
    transformer embeddings already live in `nembed` — call it, don't rebuild.

## Dependency waves (parallel execution)

Libraries in the same wave have **no dependencies on each other** → run them in parallel agents.
A wave must be green (`cargo check --workspace && cargo test --workspace`) before the next starts,
because later waves consume earlier crates.

### Wave 0 — the foundation (1 lib, run first, alone)
- ⬜ `nnum` → `niao_num` (diff 4/5) — n-dim array + linalg (LU/QR/Cholesky/SVD/eig) + FFT. **Everything below needs it.**

### Wave 1 — depend only on nnum (4 libs, fully parallel)
- ⬜ `nframe`  → `niao_frame` (diff 4/5) — DataFrame/Series, groupby/join/pivot/rolling, csv/json IO.
- ⬜ `nstats`  → `niao_stats` (diff 3/5) — distributions, hypothesis tests, correlation, OLS/GLM summaries.
- ⬜ `noptim`  → `niao_optim` (diff 3/5) — minimize (BFGS/L-BFGS/Nelder-Mead/CG), root-find, least-squares, LP.
- ⬜ `nplot`   → `niao_plot`  (diff 3/5) — line/scatter/bar/hist/heatmap/confusion → SVG (+ PNG via ncodec).

### Wave 2 — depend on Wave 0/1 (5 libs, fully parallel)
- ⬜ `nlearn`  → `niao_learn`  (diff 5/5) — estimators (linear/logistic/ridge/lasso, SVM, kNN, NB, trees,
  random forest), KMeans/DBSCAN/GMM, PCA, preprocessing, Pipeline, model_selection (CV/grid/random).
- ⬜ `nboost`  → `niao_boost`  (diff 4/5) — histogram gradient-boosted trees (reg/clf/rank), leaf-wise growth,
  early stopping, feature importance. XGBoost/LightGBM API subset.
- ⬜ `nts`     → `niao_ts`     (diff 4/5) — time series: decompose, ACF/PACF, AR/ARIMA/SARIMA, ETS/Holt-Winters,
  forecasting + intervals. statsmodels.tsa subset.
- ⬜ `nnlp`    → `niao_nlp`    (diff 4/5) — text clean, stem, n-grams, CountVectorizer/TfidfVectorizer,
  word2vec (CBOW/skip-gram), cosine sim, classical text classification.
- ⬜ `nvision` → `niao_vision` (diff 4/5) — image load/save (via ncodec), transforms (resize/crop/flip/normalize/
  augment), conv building blocks (via niao_ml), dataset loaders (MNIST/CIFAR/ImageFolder).

Legend: ⬜ to build.

## Error-code block (reserved: 4000–4099)

Highest code currently in use in the repo is ~3986, so the ML batch takes the clean **4000–4099** range,
10 codes per lib (mirrors the AI_HW / UNIQUE plans: `arity`, `error`, `type`, `invalid handle`, then lib-specific).

| Lib | Range | arity | error | type | handle/extra |
|-----|-------|:---:|:---:|:---:|---|
| `nnum`    | 4000–4009 | 4000 | 4001 | 4002 | 4003 shape/dim mismatch, 4004 singular matrix, 4005 non-convergence |
| `nframe`  | 4010–4019 | 4010 | 4011 | 4012 | 4013 bad column, 4014 length mismatch, 4015 dtype |
| `nstats`  | 4020–4029 | 4020 | 4021 | 4022 | 4023 domain (bad params), 4024 non-convergence |
| `noptim`  | 4030–4039 | 4030 | 4031 | 4032 | 4033 non-convergence, 4034 bad bounds, 4035 infeasible |
| `nplot`   | 4040–4049 | 4040 | 4041 | 4042 | 4043 invalid handle, 4044 render/encode |
| `nlearn`  | 4050–4059 | 4050 | 4051 | 4052 | 4053 not fitted, 4054 shape mismatch, 4055 non-convergence |
| `nboost`  | 4060–4069 | 4060 | 4061 | 4062 | 4063 not fitted, 4064 bad param, 4065 shape mismatch |
| `nts`     | 4070–4079 | 4070 | 4071 | 4072 | 4073 not fitted, 4074 non-stationary, 4075 non-convergence |
| `nnlp`    | 4080–4089 | 4080 | 4081 | 4082 | 4083 not fitted, 4084 empty vocab |
| `nvision` | 4090–4099 | 4090 | 4091 | 4092 | 4093 decode/encode, 4094 shape mismatch, 4095 missing file |

Each subagent uses ONLY its own block. If a lib needs a 5th code, take the next free one in its range.

> Note: `4096`/`4097` appear in the repo as buffer sizes (`[0u8; 4096]`), deflate distance codes, and LLM
> context lengths — **not** registered error codes. The 4000–4099 *error-code* namespace is clean. nvision uses
> only 4090–4095, so there is no real collision; the orchestrator confirms when wiring `codes.rs`.

## Global ground rules (every library)

1. **ZERO new third-party crates.** Only `std` + existing `niao_*` crates. No numpy/BLAS/OpenCV linkage.
2. **Lightweight + fast:** no heap allocation in hot loops; reuse buffers; pre-size `Vec`s; `#[inline]` small hot
   fns; SIMD (`std::simd` or `#[cfg]` x86/aarch64 intrinsics) **with a scalar fallback** so it builds everywhere.
3. **Reuse, don't duplicate.** Matrix ops → route through `niao_tensor` GEMM. RNG → `nrand`. Metrics → `neval`.
   Schedules/CV → `ntune`. Tokenizer → `ntok`. CSV/JSON → `ncsv`/`njson`. Image codecs → `ncodec`.
3. **Estimator trait.** nlearn, nboost, nts, nnlp all expose the same `fit(x, y) -> Self` / `predict(x)` /
   (where relevant) `transform(x)` / `score(x, y)` shape, so Pipelines and model_selection work uniformly.
4. **Expose to Niao** via `niao_libs/<name>/` wrapper (package.json + 0.2.2/lib.json + 0.2.3/lib.json,
   `kind: "native"`, correct `builtin_count`) **plus** runtime builtins — mirror `niao_libs/nvalid` exactly.
   Error codes come from this lib's reserved block only.
5. **Deliverables per lib:** crate code · `niao_libs/<name>/` wrapper · `docs/<LIB>.md` · unit tests in-crate ·
   one `examples/<lib>_demo.niao` · one `benchmarks/benchmark_<lib>.*` vs the Python/Rust reference.
6. **Correctness gate:** every algorithm is tested against a **known-good numeric reference** (values computed
   with numpy/scipy/sklearn and pasted into the test as fixtures) within a stated tolerance.
7. `cargo check --workspace` + `cargo test --workspace` green before commit. Update `CHANGELOG.md` + `REPORT.md`.
8. **Do NOT edit shared files** (`niao_libs/catalog.json`, workspace `Cargo.toml` members, `niao_runtime` wiring,
   `codes.rs`) inside a parallel agent — list needed edits in your report; the orchestrator applies them serially
   between waves to avoid merge/`Cargo.lock` conflicts.

## Numeric-correctness policy (this is an ML stack — it must be *right*)

- State a tolerance for every numeric test (e.g. `rtol=1e-5` f32, `1e-10` f64). Default dtype is **f64** for
  stats/linalg/optim; **f32** for vision/tensor interop.
- Fixed seeds everywhere (`nrand` with an explicit seed) — no flaky tests.
- Where an algorithm has a canonical result (OLS coefficients, PCA components up to sign, KMeans given a seed),
  compare to scikit-learn/scipy output committed as a fixture. Document sign/permutation ambiguities.
- Degenerate inputs return a typed error from the lib's block, never a panic or `NaN` silently.

## Performance targets (honest, std-only)

We are not trying to beat OpenBLAS/OpenCV. Targets are *usable*, not *world-record*:

- `nnum` matmul: route to `niao_tensor` GEMM (already SIMD/blocked) → within its existing target.
  Decompositions (SVD/QR/eig): correct + stable, nalgebra-class; no MKL comparison required.
- `nframe`: groupby/join within **3×** of pandas on 1M rows (columnar layout should make this reachable).
- `nlearn`/`nboost`: within **3–5×** of scikit-learn / LightGBM wall-clock on the benchmark datasets;
  identical predictions within tolerance.
- `nstats`/`noptim`/`nts`: correctness within tolerance is the gate; perf is secondary.
- `nvision` transforms: within **2×** of torchvision on a 1000-image pass.
- `nplot`: render a 10k-point chart to SVG in < 100 ms.

## Suggested build order inside each lib

1. Types + error codes (from this lib's reserved block) + the `fit/predict` (or equivalent) skeleton.
2. Core algorithm, scalar/correct first — pass the numeric-fixture tests.
3. `niao_libs/<name>/` wrapper + runtime builtins + one `.niao` example that runs end-to-end.
4. Optimize hot loops (buffer reuse, SIMD + fallback); add the benchmark; log numbers in `REPORT.md`.
5. `docs/<LIB>.md`, CHANGELOG line, final `cargo test -p <crate>` green. Report shared-file edits to orchestrator.
