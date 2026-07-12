# Niao ML libraries — Cursor rules

Copy this file to `.cursor/rules/niao-ml.mdc` (repo root) so every Cursor agent working on the ML batch
obeys Niao conventions automatically. Header block below is the .mdc frontmatter.

```mdc
---
description: Niao ML libraries (nnum/nframe/nlearn/... ) — conventions every agent must follow
globs: ["crates/niao_num/**", "crates/niao_frame/**", "crates/niao_learn/**", "crates/niao_stats/**", "crates/niao_optim/**", "crates/niao_boost/**", "crates/niao_ts/**", "crates/niao_nlp/**", "crates/niao_vision/**", "crates/niao_plot/**", "niao_libs/n*/**"]
alwaysApply: false
---

You are building native machine-learning libraries for Niao (a Rust-implemented language).

## Non-negotiable rules
- ZERO new third-party crates. Only `std` + existing `niao_*` workspace crates. No numpy/BLAS/OpenCV/torch linkage.
- Reuse existing Niao crates, never duplicate them:
  - matrix/GEMM + tensors → `niao_tensor`;  RNG → `nrand`;  arrays/linalg/FFT → `nnum`
  - metrics (accuracy/F1/RMSE/R²) → `neval`;  CV/splits/LR-schedules → `ntune`;  BPE tokenizer → `ntok`
  - neural embeddings → `nembed`;  CSV → `ncsv`;  JSON → `njson`;  image codecs → `ncodec`;  regex → `nregex`
- Lightweight + fast: no heap allocation in hot loops; reuse/pre-size buffers; `#[inline]` small hot fns;
  SIMD (`std::simd` or `#[cfg]` intrinsics) with a scalar fallback so it builds on every target.
- Numeric correctness is the gate. Every algorithm is tested against a known-good reference (numpy/scipy/
  sklearn/statsmodels/torchvision values pasted as fixtures) within a STATED tolerance. Fixed seeds — no flaky tests.
- Degenerate inputs return a typed error from THIS lib's reserved error block (4000–4099 map in MASTER_PLAN.md).
  Never panic, never return silent NaN.

## Estimator contract (nlearn, nboost, nts, nnlp)
Expose the same shape so Pipelines / model_selection compose:
`fit(x, y) -> Self` · `predict(x)` · `transform(x)` (where relevant) · `score(x, y)` (delegates to `neval`).
"Not fitted" and shape-mismatch are typed errors from the lib's block.

## Expose to Niao
Each lib ships a `niao_libs/<name>/` wrapper — `package.json` + `0.2.2/lib.json` + `0.2.3/lib.json`,
`"kind": "native"`, accurate `builtin_count`, `import_paths: ["<name>", "std/<name>"]` — mirroring
`niao_libs/nvalid` exactly, PLUS runtime builtins. Default float dtype: f64 for num/stats/optim/ts, f32 for vision.

## Deliverables per lib (all required)
crate code · `niao_libs/<name>/` wrapper · `docs/<LIB>.md` · in-crate unit tests · one `examples/<lib>_demo.niao`
· one `benchmarks/benchmark_<lib>.*` vs the Python/Rust reference · CHANGELOG line · `niao-ml-libs-plan/REPORT.md` entry.

## Do NOT touch shared files inside a parallel agent
`niao_libs/catalog.json`, workspace root `Cargo.toml` members, `niao_runtime` wiring, and `codes.rs` are edited
by the ORCHESTRATOR serially between waves. List the edits you need in your REPORT.md entry — do not apply them.

## Gate before done
`cargo check --workspace` then `cargo test -p <your_crate>` green; your `examples/<lib>_demo.niao` runs;
benchmark numbers + any deviations logged under your heading in `niao-ml-libs-plan/REPORT.md`.
```

## Dependency waves (respect the order)
- **Wave 0:** `nnum` — build first, alone. Everything depends on it.
- **Wave 1 (parallel):** `nframe`, `nstats`, `noptim`, `nplot` — each depends only on `nnum`.
- **Wave 2 (parallel):** `nlearn`, `nboost`, `nts`, `nnlp`, `nvision` — depend on Wave 0/1 crates.

Never start a wave until the previous wave is green on `cargo check --workspace && cargo test --workspace`.
