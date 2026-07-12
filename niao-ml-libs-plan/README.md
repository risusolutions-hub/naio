# niao-ml-libs-plan

Plan to add **10 native machine-learning libraries** to Niao — the scientific-ML stack that
sits around the existing `niao_tensor` + `niao_ml` (PyTorch side). These fill the
numpy / pandas / scikit-learn / scipy / statsmodels / xgboost / torchvision / nltk / matplotlib
gap with `n`-prefixed, std-only, zero-dependency Niao libraries.

## The 10 libraries

| Lib | Rust crate | Replaces (Python) | Reference (Rust) |
|-----|-----------|-------------------|------------------|
| `nnum`    | `crates/niao_num`    | numpy + scipy.linalg + scipy.fft | ndarray / nalgebra / faer |
| `nframe`  | `crates/niao_frame`  | pandas / polars | polars (Arrow columnar) |
| `nlearn`  | `crates/niao_learn`  | scikit-learn | linfa / smartcore |
| `nstats`  | `crates/niao_stats`  | scipy.stats + statsmodels | statrs |
| `noptim`  | `crates/niao_optim`  | scipy.optimize | argmin |
| `nboost`  | `crates/niao_boost`  | XGBoost / LightGBM | gbdt-rs |
| `nts`     | `crates/niao_ts`     | statsmodels.tsa / prophet | augurs |
| `nvision` | `crates/niao_vision` | torchvision / OpenCV | image-rs |
| `nnlp`    | `crates/niao_nlp`    | nltk / gensim | rust-tokenizers |
| `nplot`   | `crates/niao_plot`   | matplotlib / seaborn | plotters |

## Files in this folder

- `MASTER_PLAN.md` — the map: dependency waves, difficulty tiers, error-code block, ground rules.
- `MULTIAGENT_PROMPT.md` — **paste this into Cursor** (multi-agent/parallel mode). Orchestrator + per-agent instructions.
- `cursor-rules.md` — copy to `.cursor/rules/niao-ml.mdc` so every Cursor agent obeys Niao conventions.
- `specs/` — 10 detailed specs, one MD per library (blueprint, API surface, perf target, tests, risk).
- `cursor-tasks/` — 10 self-contained task files (one per lib) with deliverables + acceptance checklists.
- `REPORT.md` — agents append benchmark results + deviations here (one heading per lib).

## Quick start

1. Open `C:\Risu\Neko` in Cursor, enable multi-agent / parallel mode.
2. (Optional but recommended) copy `niao-ml-libs-plan/cursor-rules.md` to `.cursor/rules/niao-ml.mdc`.
3. Paste the contents of `MULTIAGENT_PROMPT.md` into the orchestrator agent.
4. It runs wave by wave — parallel agents within a wave — verifying `cargo check/test --workspace` between waves.

## Ground truth

- **Std-only, zero new third-party crates.** Only `std` + existing `niao_*` crates. SIMD with scalar fallback.
- Everything builds on `nnum` (Wave 0). Do not start Wave 1 until `nnum` is green.
- These sit **beside** `niao_tensor`/`niao_ml`, not on top — classical ML + data tooling, not another deep-learning framework.
